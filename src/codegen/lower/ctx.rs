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

use crate::abi::types::AbiType;

/// Cranelift type tag of a compiled value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValTy {
    I32,
    I64,
    F64,
    Bool,
    /// GC handle (string pool, etc.). Backed by I64. `+` faz string concat.
    Handle,
    /// Bits opacos de 64 bits (ex.: ptr/handle de namespace nao-string como
    /// `buffer.ptr`, `atomic.*`). Backed by I64. `+` faz aritmetica inteira.
    U64,
}

impl ValTy {
    pub fn cl_type(self) -> cranelift_codegen::ir::Type {
        match self {
            ValTy::I32 => cl::I32,
            ValTy::I64 | ValTy::Bool | ValTy::Handle | ValTy::U64 => cl::I64,
            ValTy::F64 => cl::F64,
        }
    }

    pub fn from_annotation(ann: &str) -> Self {
        let trimmed = ann.trim();
        match trimmed {
            "i32" | "I32" => return ValTy::I32,
            "f64" | "F64" | "number" => return ValTy::F64,
            "bool" | "boolean" => return ValTy::Bool,
            "string" | "str" => return ValTy::Handle,
            _ => {}
        }
        // Union types raw da source: tenta resolver para um tipo unico
        // se todos os ramos sao do mesmo. Usa parsing textual simples.
        if trimmed.contains('|') {
            let parts: Vec<&str> = trimmed
                .split('|')
                .map(|s| s.trim().trim_start_matches('(').trim_end_matches(')').trim())
                .filter(|s| !s.is_empty() && *s != "null" && *s != "undefined")
                .collect();
            if !parts.is_empty() {
                let mut acc: Option<ValTy> = None;
                for p in parts {
                    // Cada parte: pode ser keyword, string literal "...",
                    // numero literal, etc.
                    let cur = if p.starts_with('"') && p.ends_with('"') {
                        ValTy::Handle
                    } else if p.starts_with('\'') && p.ends_with('\'') {
                        ValTy::Handle
                    } else if p.parse::<f64>().is_ok() {
                        ValTy::I64
                    } else {
                        ValTy::from_annotation(p)
                    };
                    match acc {
                        None => acc = Some(cur),
                        Some(prev) if prev == cur => {}
                        _ => return ValTy::I64,
                    }
                }
                if let Some(t) = acc {
                    return t;
                }
            }
        }
        ValTy::I64
    }

    pub fn from_abi(abi: AbiType) -> Self {
        match abi {
            AbiType::I32 => ValTy::I32,
            AbiType::F64 => ValTy::F64,
            AbiType::Bool => ValTy::Bool,
            AbiType::Handle => ValTy::Handle,
            AbiType::U64 => ValTy::U64,
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
    /// Class name when the function's declared return type is a known class.
    /// Populated by `compile_program` after class declarations are collected.
    pub ret_class: Option<String>,
}

/// Decide se o codegen deve usar layout nativo (flat) para uma classe.
///
/// Mecanismo opt-in para a fase de coexistencia da #147 — fora dos casos
/// abaixo o codegen continua bit-identico ao caminho `Map`-based.
///
/// Gatilhos (qualquer um ativa):
/// - Nome da classe comeca com `__Flat` — permite teste sem env vars
///   (`rts:test` nao propaga env). Documentado no helper para que classes
///   de proucao nao sejam afetadas acidentalmente.
/// - Variavel de ambiente `RTS_FLAT_CLASSES` lista o nome (csv).
///
/// `RTS_FLAT_CLASSES=Point,Color cargo run -- run x.ts` ativa flat para
/// `Point` e `Color`. Vazio/ausente = nenhuma classe flat por env.
pub fn is_class_flat_enabled(name: &str) -> bool {
    if name.starts_with("__Flat") {
        return true;
    }
    let Ok(list) = std::env::var("RTS_FLAT_CLASSES") else {
        return false;
    };
    list.split(',').any(|n| n.trim() == name)
}

/// Slot de um campo numa classe com layout nativo.
///
/// Cada campo ocupa 8 bytes (alinhamento simples). `is_handle` marca os
/// slots que carregam GC handles (strings, instancias) — necessario para
/// trace futuro do GC.
#[derive(Debug, Clone)]
pub struct FieldSlot {
    pub name: String,
    pub offset: u32,
    pub ty: ValTy,
    pub is_handle: bool,
}

/// Layout nativo computado para uma classe elegivel.
///
/// Slot 0 (`offset=0`) sempre reservado para o tag `__rts_class` (handle
/// de string), por isso `parent_size` minimo é 8 quando nao ha super.
/// Quando ha super, os campos desta classe comecam em `parent_size`
/// (que ja inclui o slot do tag).
#[derive(Debug, Clone)]
pub struct ClassLayout {
    pub fields: Vec<FieldSlot>,
    pub size_bytes: u32,
    pub parent_size: u32,
}

/// Metadata estatica de uma classe TS/JS, consumida pelo codegen para
/// resolver `new C(...)`, `obj.method(...)` e dispatch de `super`.
///
/// Usado em compile-time apenas — runtime nao conhece classes, apenas
/// handles de map/vec.
#[derive(Debug, Clone)]
pub struct ClassMeta {
    pub name: String,
    pub super_class: Option<String>,
    /// Nomes de metodos definidos diretamente nesta classe (instance).
    pub methods: Vec<String>,
    /// Tipo declarado de cada field (via `class { x: string }`). Usado
    /// para tipar o resultado de `obj.field` quando a classe da var e
    /// conhecida.
    pub field_types: HashMap<String, ValTy>,
    /// Nome textual cru do tipo de cada field (ex: `"Vec3"`, `"i32"`).
    /// Permite descobrir quando o field e instancia de outra classe
    /// registrada — habilita overload em `this.field + x`.
    pub field_class_names: HashMap<String, String>,
    /// Nomes de static methods (chamados via `C.method()`).
    pub static_methods: Vec<String>,
    /// Nomes de static fields (acessados via `C.field`).
    pub static_fields: Vec<String>,
    /// Nomes de getters definidos diretamente — ler `obj.x` chama
    /// __class_C_get_x(this).
    pub getters: Vec<String>,
    /// Nomes de setters definidos diretamente — `obj.x = v` chama
    /// __class_C_set_x(this, v).
    pub setters: Vec<String>,
    /// True quando a classe tem constructor proprio (mesmo se vazio).
    pub has_constructor: bool,
    /// Nomes de fields marcados `readonly` — só podem ser atribuídos
    /// dentro do constructor. Reassign em outros métodos é erro.
    pub readonly_fields: std::collections::HashSet<String>,
    /// Visibility declarada de cada membro (field ou método) que não
    /// é \`public\`. \`public\` é default e não é registrado pra economizar.
    /// Acessos a \`obj.x\` quando \`x\` é \`private\`/\`protected\` são
    /// validados em compile-time conforme regras TS.
    pub member_visibility: std::collections::HashMap<String, crate::parser::ast::Visibility>,
    /// `abstract class C` — não pode ser instanciada via `new C()`.
    pub is_abstract: bool,
    /// Nomes de métodos abstract declarados nesta classe (sem implementação).
    /// Subclasses concretas devem implementar todos os abstract methods
    /// herdados (incluindo de ancestrais).
    pub abstract_methods: std::collections::HashSet<String>,
    /// Layout nativo computado, quando a classe é elegível (todos os
    /// fields anotados, sem getters/setters). `None` para classes que
    /// continuam usando o caminho `Map`-based atual. Populado em segundo
    /// pass de `compile_program` apos coletar todas as ClassMeta.
    pub layout: Option<ClassLayout>,
}

/// Per-function compilation context.
///
/// `module` is stored as `&mut dyn Module` so the same codegen plumbing
/// serves both `ObjectModule` (AOT via `rts compile`) and `JITModule`
/// (in-memory via `rts run`). The Module trait is object-safe — every
/// method we need dispatches through the vtable without Self-bounds.
pub struct FnCtx<'m, 'fb> {
    pub builder: &'fb mut FunctionBuilder<'m>,
    pub module: &'m mut dyn Module,
    pub extern_cache: &'fb mut HashMap<&'static str, cranelift_module::FuncId>,
    pub data_counter: &'fb mut u32,

    /// Stack of scopes. The first entry is the function scope (where `var`
    /// declarations live); subsequent entries are block scopes for `let`/`const`.
    pub locals: Vec<HashMap<String, LocalVar>>,
    /// Module-scope globals visible from functions.
    pub globals: &'fb HashMap<String, GlobalVar>,
    /// User-defined function signatures by source name.
    pub user_fns: &'fb HashMap<String, UserFnAbi>,
    /// Classe retornada por user functions cujo return_type bate com
    /// uma classe registrada. Permite `const x: V = makeV()` e
    /// `this.doubled() + this.doubled()` detectarem o tipo.
    pub fn_class_returns: &'fb HashMap<String, String>,
    /// Classes registradas no programa, indexadas pelo nome da classe.
    /// Permite resolver `new C(args)`, `super(args)` e `super.method(args)`
    /// em compile-time sem vtable.
    pub classes: &'fb HashMap<String, ClassMeta>,
    /// Tipo estatico declarado de cada local conhecido como instancia
    /// de classe — povoado quando a anotacao do bind e `: ClassName`.
    /// Permite dispatch estatico de `obj.method(...)`.
    pub local_class_ty: HashMap<String, String>,
    /// Quando o local e array tipado `: ClassName[]`, guarda o nome
    /// da classe dos elementos. Usado para inferir tipo de bind em
    /// for-of e em `arr[i]`.
    pub local_array_class_ty: HashMap<String, String>,
    /// Tipo dos campos de uma var que e object literal (ex: enum string,
    /// `const E = { Red: "red" }`). Permite que `E.Red` retorne Handle
    /// em vez de I64 anonimo.
    pub local_obj_field_types: HashMap<String, HashMap<String, ValTy>>,
    /// Tipo estatico de globais module-scope que sao instancias de
    /// classe. Populado uma vez em compile_program e compartilhado
    /// entre todos os FnCtx — permite dispatch de overload em funcoes
    /// top-level que referenciam `const a: V = new V(...)` global.
    pub global_class_ty: &'fb HashMap<String, String>,
    /// Nome da classe atualmente sendo lowered (quando dentro de um
    /// metodo ou constructor). Usado para resolver `super`.
    pub current_class: Option<String>,
    /// True quando a função atual é um constructor de classe
    /// (`__class_C__init`). Usado pra permitir assign em readonly fields.
    pub current_is_ctor: bool,
    /// True when lowering top-level statements in `main`.
    pub module_scope: bool,
    /// Declared return type of the surrounding function, used to coerce
    /// `return <expr>` to the correct Cranelift type.
    pub return_ty: Option<ValTy>,
    /// True while lowering the expression of a `return` statement — enables
    /// tail-call optimisation in `lower_user_call` when the surrounding
    /// function uses the Tail calling convention.
    pub in_tail_position: bool,
    /// True when the function being lowered uses `CallConv::Tail`. Only
    /// tail-conv callers can emit `return_call` to tail-conv callees.
    /// `__RTS_MAIN` stays on the platform default and keeps this false.
    pub is_tail_conv: bool,

    /// Cranelift variable counter.
    var_counter: u32,

    /// Stack of (break_block, continue_block, optional_label) for nested
    /// loops. Label permite \`break LABEL\` saltar para um loop externo
    /// específico em vez do mais interno.
    pub loop_stack: Vec<(Block, Block, Option<String>)>,
    /// Quando \`Stmt::Labeled\` envolve o próximo loop, registra aqui o
    /// nome do label. O loop seguinte consome (via \`take()\`) ao fazer
    /// push no \`loop_stack\`.
    pub pending_label: Option<String>,
}

impl<'m, 'fb> FnCtx<'m, 'fb> {
    pub fn new(
        builder: &'fb mut FunctionBuilder<'m>,
        module: &'m mut dyn Module,
        extern_cache: &'fb mut HashMap<&'static str, cranelift_module::FuncId>,
        data_counter: &'fb mut u32,
        globals: &'fb HashMap<String, GlobalVar>,
        user_fns: &'fb HashMap<String, UserFnAbi>,
        classes: &'fb HashMap<String, ClassMeta>,
        global_class_ty: &'fb HashMap<String, String>,
        fn_class_returns: &'fb HashMap<String, String>,
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
            fn_class_returns,
            classes,
            local_class_ty: HashMap::new(),
            local_array_class_ty: HashMap::new(),
            local_obj_field_types: HashMap::new(),
            global_class_ty,
            current_class: None,
            current_is_ctor: false,
            module_scope,
            return_ty: None,
            in_tail_position: false,
            is_tail_conv: false,
            var_counter: 0,
            loop_stack: Vec::new(),
            pending_label: None,
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
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
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
        let idx = if function_scope {
            0
        } else {
            self.locals.len() - 1
        };
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
            ValTy::U64 => Ok(TypedVal::new(tv.val, ValTy::Handle)),
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
            ValTy::I64 | ValTy::Bool | ValTy::Handle | ValTy::U64 => tv,
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
            ValTy::I64 | ValTy::Bool | ValTy::Handle | ValTy::U64 => {
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
            ValTy::I64 | ValTy::Bool | ValTy::Handle | ValTy::U64 => TypedVal::new(
                self.builder.ins().fcvt_from_sint(cl::F64, tv.val),
                ValTy::F64,
            ),
        }
    }

    /// Returns the current loop break target, if any. Sem label,
    /// usa o loop mais interno; com label, busca pelo label.
    pub fn break_block(&self) -> Option<Block> {
        self.loop_stack.last().map(|(brk, _, _)| *brk)
    }

    /// Returns the current loop continue target, if any.
    pub fn continue_block(&self) -> Option<Block> {
        self.loop_stack.last().map(|(_, cont, _)| *cont)
    }

    /// Resolve break para um label específico — busca no stack do
    /// topo até a base. Returns None se não encontrar.
    pub fn break_block_for_label(&self, label: &str) -> Option<Block> {
        for (brk, _, lbl) in self.loop_stack.iter().rev() {
            if lbl.as_deref() == Some(label) {
                return Some(*brk);
            }
        }
        None
    }

    /// Resolve continue para um label específico.
    pub fn continue_block_for_label(&self, label: &str) -> Option<Block> {
        for (_, cont, lbl) in self.loop_stack.iter().rev() {
            if lbl.as_deref() == Some(label) {
                return Some(*cont);
            }
        }
        None
    }
}
