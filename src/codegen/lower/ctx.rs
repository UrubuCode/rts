//! Per-function compilation context.
//!
//! `FnCtx` wraps a Cranelift `FunctionBuilder` and carries the bookkeeping
//! needed to compile one function: local/global variables, extern cache,
//! string literal data, and loop control targets.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{
    Block, FuncRef, InstBuilder, MemFlags, StackSlot, StackSlotData, StackSlotKind, Value,
    types as cl,
};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::{DataId, Module};
use cranelift_object::ObjectModule;

use crate::abi::types::AbiType;

/// Cranelift type tag of a compiled value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValTy {
    I32,
    I64,
    F64,
    Bool,
    /// GC handle. Backed by I64.
    Handle,
}

impl ValTy {
    pub fn cl_type(self) -> cranelift_codegen::ir::Type {
        match self {
            ValTy::I32 => cl::I32,
            ValTy::I64 | ValTy::Bool | ValTy::Handle => cl::I64,
            ValTy::F64 => cl::F64,
        }
    }

    pub fn from_annotation(ann: &str) -> Self {
        match ann.trim() {
            "i32" | "I32" => ValTy::I32,
            "f64" | "F64" | "number" => ValTy::F64,
            "bool" | "boolean" => ValTy::Bool,
            "string" | "str" => ValTy::Handle,
            _ => ValTy::I64,
        }
    }

    pub fn from_abi(abi: AbiType) -> Self {
        match abi {
            AbiType::I32 => ValTy::I32,
            AbiType::F64 => ValTy::F64,
            AbiType::Bool => ValTy::Bool,
            AbiType::Handle | AbiType::U64 => ValTy::Handle,
            _ => ValTy::I64,
        }
    }
}

/// A typed Cranelift value.
#[derive(Debug, Clone, Copy)]
pub struct TypedVal {
    pub val: Value,
    pub ty: ValTy,
}

impl TypedVal {
    pub fn new(val: Value, ty: ValTy) -> Self {
        Self { val, ty }
    }
}

/// Slot for a local variable.
#[derive(Debug, Clone)]
pub struct LocalVar {
    pub var: Variable,
    pub ty: ValTy,
    /// True when declared with `const` — reassignment must be rejected.
    pub is_const: bool,
}

/// Module-scope global lowered to a data symbol.
#[derive(Debug, Clone)]
pub struct GlobalVar {
    pub data_id: DataId,
    pub ty: ValTy,
}

/// Callable user function signature visible while lowering calls.
#[derive(Debug, Clone)]
pub struct UserFnAbi {
    pub params: Vec<ValTy>,
    pub ret: Option<ValTy>,
}

/// Per-function compilation context.
pub struct FnCtx<'m, 'fb> {
    pub builder: &'fb mut FunctionBuilder<'m>,
    pub module: &'m mut ObjectModule,
    pub extern_cache: &'fb mut HashMap<&'static str, cranelift_module::FuncId>,
    pub data_counter: &'fb mut u32,

    /// Stack of scopes. The first entry is the function scope (where `var`
    /// declarations live); subsequent entries are block scopes for `let`/`const`.
    pub locals: Vec<HashMap<String, LocalVar>>,
    /// Module-scope globals visible from functions.
    pub globals: &'fb HashMap<String, GlobalVar>,
    /// User-defined function signatures by source name.
    pub user_fns: &'fb HashMap<String, UserFnAbi>,
    /// True when lowering top-level statements in `main`.
    pub module_scope: bool,
    /// Declared return type of the surrounding function, used to coerce
    /// `return <expr>` to the correct Cranelift type.
    pub return_ty: Option<ValTy>,

    /// Cranelift variable counter.
    var_counter: u32,

    /// Stack of (break_block, continue_block) for nested loops.
    pub loop_stack: Vec<(Block, Block)>,
}

impl<'m, 'fb> FnCtx<'m, 'fb> {
    pub fn new(
        builder: &'fb mut FunctionBuilder<'m>,
        module: &'m mut ObjectModule,
        extern_cache: &'fb mut HashMap<&'static str, cranelift_module::FuncId>,
        data_counter: &'fb mut u32,
        globals: &'fb HashMap<String, GlobalVar>,
        user_fns: &'fb HashMap<String, UserFnAbi>,
        module_scope: bool,
    ) -> Self {
        Self {
            builder,
            module,
            extern_cache,
            data_counter,
            locals: vec![HashMap::new()],
            globals,
            user_fns,
            module_scope,
            return_ty: None,
            var_counter: 0,
            loop_stack: Vec::new(),
        }
    }

    /// Allocates a new Cranelift variable slot.
    pub fn new_var(&mut self, ty: ValTy) -> Variable {
        self.var_counter += 1;
        self.builder.declare_var(ty.cl_type())
    }

    /// Allocates an explicit stack slot of `size` bytes with the given
    /// log2 alignment. Returns the slot handle and a pointer to its base.
    ///
    /// Use when a value needs a stable address — scenarios that
    /// [`Variable`] (pure SSA) cannot model:
    ///
    /// - **Mutable captures in closures** (#97 — `() => x++` needs `&mut x`)
    /// - **Aggregate return values** (tuple/struct returns, future)
    /// - **Fixed-size local arrays** (`const buf = [0, 0, 0]`)
    ///
    /// Loads and stores through the returned pointer should use
    /// [`MemFlags::trusted()`] — stack slots are always aligned and
    /// addressable. Prefer [`new_var`](Self::new_var) for plain scalars
    /// that do not need an address: `Variable` is cheaper and lowers to
    /// a single Cranelift value without going through memory.
    ///
    /// Until #97 lands there is no consumer inside this crate; the helper
    /// is kept `pub` so the closure lowering work has a small surface to
    /// plug into.
    pub fn alloc_stack_slot(&mut self, size: u32, align_log2: u8) -> (StackSlot, Value) {
        let slot = self
            .builder
            .create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                size,
                align_log2,
            ));
        let ptr_ty = self.module.isa().pointer_type();
        let addr = self.builder.ins().stack_addr(ptr_ty, slot, 0);
        (slot, addr)
    }

    /// Pushes a new block scope. Variables declared with `let`/`const` go here.
    pub fn push_scope(&mut self) {
        self.locals.push(HashMap::new());
    }

    /// Pops the current block scope.
    pub fn pop_scope(&mut self) {
        if self.locals.len() > 1 {
            self.locals.pop();
        }
    }

    /// Declares a named local in the current (top-most) scope.
    pub fn declare_local(&mut self, name: &str, ty: ValTy, init: Value) {
        self.declare_local_kind(name, ty, init, false, false);
    }

    /// Declares a named local with control over mutability and scope target.
    ///
    /// `function_scope = true` places the binding in the function-level scope
    /// (used for `var`); otherwise it lands in the current block scope.
    pub fn declare_local_kind(
        &mut self,
        name: &str,
        ty: ValTy,
        init: Value,
        is_const: bool,
        function_scope: bool,
    ) {
        let var = self.new_var(ty);
        self.builder.def_var(var, init);
        let slot = LocalVar { var, ty, is_const };
        let idx = if function_scope { 0 } else { self.locals.len() - 1 };
        self.locals[idx].insert(name.to_string(), slot);
    }

    fn find_local(&self, name: &str) -> Option<LocalVar> {
        for scope in self.locals.iter().rev() {
            if let Some(slot) = scope.get(name) {
                return Some(slot.clone());
            }
        }
        None
    }

    /// Reads a named local or module global.
    pub fn read_local(&mut self, name: &str) -> Option<TypedVal> {
        if let Some(local) = self.find_local(name) {
            let val = self.builder.use_var(local.var);
            return Some(TypedVal::new(val, local.ty));
        }

        let global = self.globals.get(name).cloned()?;
        let gv = self
            .module
            .declare_data_in_func(global.data_id, self.builder.func);
        let ptr = self
            .builder
            .ins()
            .global_value(self.module.isa().pointer_type(), gv);
        // Module globals are declared with natural alignment (4/8 bytes)
        // and always initialised before first read (top-level emits the
        // initialiser before any reader). `trusted()` = aligned + notrap
        // lets the backend pick the widest load instruction.
        let val = self
            .builder
            .ins()
            .load(global.ty.cl_type(), MemFlags::trusted(), ptr, 0);
        Some(TypedVal::new(val, global.ty))
    }

    /// Writes to a named local or module global.
    pub fn write_local(&mut self, name: &str, val: Value) -> Result<()> {
        if let Some(local) = self.find_local(name) {
            if local.is_const {
                return Err(anyhow!("assignment to const variable `{name}`"));
            }
            self.builder.def_var(local.var, val);
            return Ok(());
        }

        if let Some(global) = self.globals.get(name).cloned() {
            let gv = self
                .module
                .declare_data_in_func(global.data_id, self.builder.func);
            let ptr = self
                .builder
                .ins()
                .global_value(self.module.isa().pointer_type(), gv);
            let casted = self.coerce_value_to_ty(val, global.ty);
            self.builder
                .ins()
                .store(MemFlags::trusted(), casted, ptr, 0);
            return Ok(());
        }

        Err(anyhow!("assignment to undeclared variable `{name}`"))
    }

    /// Returns the declared type of a local/global variable.
    pub fn var_ty(&self, name: &str) -> Option<ValTy> {
        self.find_local(name)
            .map(|local| local.ty)
            .or_else(|| self.globals.get(name).map(|global| global.ty))
    }

    /// Returns true if `name` is a module global.
    pub fn has_global(&self, name: &str) -> bool {
        self.globals.contains_key(name)
    }

    fn coerce_value_to_ty(&mut self, value: Value, ty: ValTy) -> Value {
        let expected = ty.cl_type();
        let actual = self.builder.func.dfg.value_type(value);
        if actual == expected {
            return value;
        }

        match (actual, expected) {
            (cl::I32, cl::I64) => self.builder.ins().sextend(cl::I64, value),
            (cl::I64, cl::I32) => self.builder.ins().ireduce(cl::I32, value),
            (cl::I32, cl::F64) => self.builder.ins().fcvt_from_sint(cl::F64, value),
            (cl::I64, cl::F64) => self.builder.ins().fcvt_from_sint(cl::F64, value),
            (cl::F64, cl::I64) => self.builder.ins().fcvt_to_sint_sat(cl::I64, value),
            (cl::F64, cl::I32) => {
                let as_i64 = self.builder.ins().fcvt_to_sint_sat(cl::I64, value);
                self.builder.ins().ireduce(cl::I32, as_i64)
            }
            _ => value,
        }
    }

    /// Ensures an extern symbol is declared and returns a FuncRef.
    pub fn get_extern(
        &mut self,
        symbol: &'static str,
        params: &[cranelift_codegen::ir::Type],
        ret: Option<cranelift_codegen::ir::Type>,
    ) -> Result<FuncRef> {
        if !self.extern_cache.contains_key(symbol) {
            use cranelift_codegen::ir::{AbiParam, Signature};
            use cranelift_module::Linkage;

            let mut sig = Signature::new(self.module.isa().default_call_conv());
            for &p in params {
                sig.params.push(AbiParam::new(p));
            }
            if let Some(r) = ret {
                sig.returns.push(AbiParam::new(r));
            }
            let id = self
                .module
                .declare_function(symbol, Linkage::Import, &sig)
                .map_err(|e| anyhow!("failed to declare extern {symbol}: {e}"))?;
            self.extern_cache.insert(symbol, id);
        }
        let id = *self.extern_cache.get(symbol).expect("extern declared");
        Ok(self.module.declare_func_in_func(id, self.builder.func))
    }

    /// Emits a rodata string literal and returns (ptr: i64, len: i64).
    pub fn emit_str_literal(&mut self, bytes: &[u8]) -> Result<(Value, Value)> {
        use cranelift_module::{DataDescription, Linkage};

        let name = format!(".Lrts_str_{}", self.data_counter);
        *self.data_counter += 1;

        let data_id = self
            .module
            .declare_data(&name, Linkage::Local, false, false)
            .map_err(|e| anyhow!("failed to declare data {name}: {e}"))?;

        let mut desc = DataDescription::new();
        desc.define(bytes.to_vec().into_boxed_slice());
        self.module
            .define_data(data_id, &desc)
            .map_err(|e| anyhow!("failed to define data {name}: {e}"))?;

        let gv = self.module.declare_data_in_func(data_id, self.builder.func);
        let ptr_ty = self.module.isa().pointer_type();
        let ptr = self.builder.ins().global_value(ptr_ty, gv);
        let ptr = if ptr_ty == cl::I64 {
            ptr
        } else {
            self.builder.ins().uextend(cl::I64, ptr)
        };
        let len = self.builder.ins().iconst(cl::I64, bytes.len() as i64);
        Ok((ptr, len))
    }

    /// Promotes a static string literal to a GC handle.
    pub fn emit_str_handle(&mut self, bytes: &[u8]) -> Result<TypedVal> {
        let (ptr, len) = self.emit_str_literal(bytes)?;
        let fref = self.get_extern(
            "__RTS_FN_NS_GC_STRING_FROM_STATIC",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let inst = self.builder.ins().call(fref, &[ptr, len]);
        let val = self.builder.inst_results(inst)[0];
        Ok(TypedVal::new(val, ValTy::Handle))
    }

    /// Coerces any value to a GC string handle.
    pub fn coerce_to_handle(&mut self, tv: TypedVal) -> Result<TypedVal> {
        match tv.ty {
            ValTy::Handle => Ok(tv),
            ValTy::I64 | ValTy::I32 | ValTy::Bool => {
                let as_i64 = self.coerce_to_i64(tv);
                let fref =
                    self.get_extern("__RTS_FN_NS_GC_STRING_FROM_I64", &[cl::I64], Some(cl::I64))?;
                let inst = self.builder.ins().call(fref, &[as_i64.val]);
                let val = self.builder.inst_results(inst)[0];
                Ok(TypedVal::new(val, ValTy::Handle))
            }
            ValTy::F64 => {
                let fref =
                    self.get_extern("__RTS_FN_NS_GC_STRING_FROM_F64", &[cl::F64], Some(cl::I64))?;
                let inst = self.builder.ins().call(fref, &[tv.val]);
                let val = self.builder.inst_results(inst)[0];
                Ok(TypedVal::new(val, ValTy::Handle))
            }
        }
    }

    /// Coerces a value to I64.
    pub fn coerce_to_i64(&mut self, tv: TypedVal) -> TypedVal {
        match tv.ty {
            ValTy::I64 | ValTy::Bool | ValTy::Handle => tv,
            ValTy::I32 => TypedVal::new(self.builder.ins().sextend(cl::I64, tv.val), ValTy::I64),
            ValTy::F64 => TypedVal::new(
                self.builder.ins().fcvt_to_sint_sat(cl::I64, tv.val),
                ValTy::I64,
            ),
        }
    }

    /// Coerces a value to I32.
    pub fn coerce_to_i32(&mut self, tv: TypedVal) -> TypedVal {
        match tv.ty {
            ValTy::I32 => tv,
            ValTy::I64 | ValTy::Bool | ValTy::Handle => {
                TypedVal::new(self.builder.ins().ireduce(cl::I32, tv.val), ValTy::I32)
            }
            ValTy::F64 => {
                let as_i64 = self.builder.ins().fcvt_to_sint_sat(cl::I64, tv.val);
                TypedVal::new(self.builder.ins().ireduce(cl::I32, as_i64), ValTy::I32)
            }
        }
    }

    /// Coerces a value to F64.
    pub fn coerce_to_f64(&mut self, tv: TypedVal) -> TypedVal {
        match tv.ty {
            ValTy::F64 => tv,
            ValTy::I32 => TypedVal::new(
                self.builder.ins().fcvt_from_sint(cl::F64, tv.val),
                ValTy::F64,
            ),
            ValTy::I64 | ValTy::Bool | ValTy::Handle => TypedVal::new(
                self.builder.ins().fcvt_from_sint(cl::F64, tv.val),
                ValTy::F64,
            ),
        }
    }

    /// Returns the current loop break target, if any.
    pub fn break_block(&self) -> Option<Block> {
        self.loop_stack.last().map(|(brk, _)| *brk)
    }

    /// Returns the current loop continue target, if any.
    pub fn continue_block(&self) -> Option<Block> {
        self.loop_stack.last().map(|(_, cont)| *cont)
    }
}
