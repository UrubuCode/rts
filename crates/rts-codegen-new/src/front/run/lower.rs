//! Whole-program HIR → Cranelift lowering: the Tagged/polymorphic path.
//!
//! Where [`crate::front::hir_lower`] is the strict numeric-only fast path (used
//! by the increment-3 tests, unchanged), this lowerer adds the `Tagged`
//! [`crate::value::PolyValue`] path so a REAL program — top-level code, string
//! literals, `console.log`, `typeof`, `===`, cross-function calls — runs end to
//! end. Proven-numeric operands still use native `iadd`/`fadd`/… (the fast path
//! is NOT regressed); a value boxes to a `PolyValue` only at a Tagged boundary
//! (a `console.log` arg, a `+` on a string/mixed pair, a Tagged local, a Tagged
//! call argument), via the pure `value::emit_*` IR helpers and the generic
//! `__rtsn_*` runtime ops.
//!
//! Every unmodeled construct is an EXPLICIT [`Unsupported`] bail (design pilar 2):
//! the program runner refuses to run a partially-lowered module rather than emit
//! a wrong value.
//!
//! Submodules: this file holds the driver + function shell + local/coercion
//! scaffolding; [`super::expr`] lowers expressions, [`super::stmt`] statements +
//! control flow. Each stays well under 500 lines.

use std::collections::HashMap;

use cranelift_codegen::ir::{types, InstBuilder, Value};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::Module;

use rts_hir::{HirFunc, HirStmt};

use crate::repr::Repr;
use crate::shape::{ShapeId, ShapeTable};
use crate::value;

use crate::front::error::{unsupported, FrontResult, Unsupported};

use super::sig::FnSig;

/// The Cranelift type a value of `repr` is carried in: `Float64` in an `f64`,
/// every unboxed-integer / `Bool` / `Tagged` value in an `i64` register.
pub fn cl_type(repr: Repr) -> types::Type {
    match repr {
        Repr::Float64 => types::F64,
        _ => types::I64,
    }
}

/// A proven JS *type-kind* hint, finer than [`Repr`] for the cases where the
/// HIR has thrown away operator distinctions the engine must respect soundly.
///
/// Concretely: swc maps BOTH `==` and `===` to one HIR op, and BOTH unary `+`
/// and `!` to one HIR op — so the engine cannot tell them apart from the HIR.
/// Strict equality and ToBoolean-not are only sound to lower when the operand
/// kinds make `==`/`===` (resp. `+`/`!`) agree. `JsKind` records the kind where
/// it is statically provable (literals, comparison results) so the lowering can
/// keep the sound cases and BAIL the ambiguous ones — never emit a wrong value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum JsKind {
    Number,
    Bool,
    Str,
    Null,
    Undefined,
    /// A whole ARRAY value (a `TAG_OBJECT` PolyValue over a real `Entry::Vec`),
    /// produced by an array-returning method (e.g. `arr.slice(..)`). Lets a `let`
    /// record [`HeapShape::Array`] so the new local supports `.length`/`[i]`/
    /// array methods.
    Array,
    /// A FUNCTION value (a `TAG_FUNCTION` PolyValue over a reified `Entry::Function`
    /// holding the thunk address). Produced by reifying a user-fn ident or an
    /// extracted arrow (P4.6); makes `f(args)` lower to the indirect invoke path
    /// and `typeof f` fold to `"function"`.
    Function,
    /// A class-instance OBJECT value (a `TAG_OBJECT` PolyValue over the instance
    /// Vec). Produced by `new C(args)` (P4.9); makes a `let` record the local's
    /// class + OBJECT shape and routes `console.log` through the object inspect.
    Object,
    /// Not statically provable (a Tagged value from a variable/call/etc.).
    Unknown,
}

/// An SSA value tagged with the representation it carries + a static kind hint.
#[derive(Clone, Copy)]
pub(crate) struct Val {
    pub v: Value,
    pub repr: Repr,
    pub kind: JsKind,
}

impl Val {
    /// A value whose kind follows from its repr (number/bool reprs are
    /// self-describing; a Tagged value's kind is `Unknown` unless set explicitly).
    pub(crate) fn new(v: Value, repr: Repr) -> Val {
        let kind = match repr {
            Repr::Float64 | Repr::Int32 | Repr::Int64 => JsKind::Number,
            Repr::Bool => JsKind::Bool,
            _ => JsKind::Unknown,
        };
        Val { v, repr, kind }
    }

    /// A Tagged value with an explicit proven kind (string/null/undefined literal).
    pub(crate) fn tagged_kind(v: Value, kind: JsKind) -> Val {
        Val { v, repr: Repr::Tagged, kind }
    }
}

/// A local binding: its Cranelift variable + the repr it holds. A local has a
/// single stable repr for the whole function; a `let` that would need two reprs
/// on different paths uses `Tagged` (decided at declaration).
#[derive(Clone, Copy)]
pub(crate) struct Local {
    pub var: Variable,
    pub repr: Repr,
}

/// The statically-proven heap shape of a local that holds an object or array
/// literal — what makes `obj.key` / `arr[i]` / `arr.length` lowerable to a
/// constant-slot `VEC_GET`/`VEC_SET`. A local without an entry here holds an
/// opaque value (param, call return, reassigned); an access on it BAILS.
#[derive(Clone, Copy)]
pub(crate) enum HeapShape {
    /// An object literal with a known compile-time [`ShapeId`] (key→slot map).
    Object(ShapeId),
    /// An array literal: indices are dense `0..len`; `.length` is `VEC_LEN`.
    Array,
}

/// The per-function lowering context.
pub(crate) struct Lowerer<'a, 'b, 'c> {
    pub builder: &'a mut FunctionBuilder<'b>,
    /// Name → local binding (flat; re-`let` reuses the slot).
    pub locals: HashMap<String, Local>,
    /// Name → the proven heap shape of a local holding an object/array literal.
    /// Absent ⇒ the local's value is opaque; a property/index access on it bails.
    pub local_shapes: HashMap<String, HeapShape>,
    /// Name → the statically-known CLASS of a local/param holding a class instance
    /// (a `new C()` result, a `: C`-annotated param, or `this` inside a method).
    /// Drives static `instance.method(args)` dispatch; absent ⇒ method calls bail.
    pub local_classes: HashMap<String, String>,
    /// Interns object-literal shapes (compile-time key→slot maps) for this fn.
    pub shapes: ShapeTable,
    /// The function's return repr, or `None` for a `void` body (`__rtsn_main`).
    pub ret: Option<Repr>,
    /// True once the current block has emitted a terminator.
    pub block_terminated: bool,
    /// Every user function's frozen ABI signature, for cross-fn calls. Keyed by
    /// name; shared (read-only) across all functions in the module.
    pub sigs: &'c HashMap<String, FnSig>,
    /// Each user function's uniform-ABI THUNK FuncId, for reifying a function
    /// referenced as a VALUE (`func_addr` of the thunk → `__rtsadp_fn_reify`).
    pub thunks: &'c HashMap<String, cranelift_module::FuncId>,
    /// The program's user classes (descriptors: fields → shape slots, methods →
    /// functions). Read-only, shared across all functions. Drives `new C(args)`,
    /// `this.field`, and static `instance.method(args)`.
    pub classes: &'c super::class::ClassTable,
}

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Lower one user function (or the synthesized main) into `builder`, whose
    /// `Function` already carries the signature from [`FnSig::to_cranelift`].
    #[allow(clippy::too_many_arguments)]
    pub fn lower_function(
        module: &mut dyn Module,
        builder: &'a mut FunctionBuilder<'b>,
        func: &HirFunc,
        sig: &FnSig,
        sigs: &'c HashMap<String, FnSig>,
        thunks: &'c HashMap<String, cranelift_module::FuncId>,
        classes: &'c super::class::ClassTable,
        this_class: Option<&str>,
    ) -> FrontResult<()> {
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut ctx = Lowerer {
            builder,
            locals: HashMap::new(),
            local_shapes: HashMap::new(),
            local_classes: HashMap::new(),
            shapes: ShapeTable::new(),
            ret: sig.ret,
            block_terminated: false,
            sigs,
            thunks,
            classes,
        };

        // Bind each parameter to a fresh local Variable carrying its ABI repr.
        for (i, (p, &repr)) in func.params.iter().zip(&sig.params).enumerate() {
            let block_val = ctx.builder.block_params(entry)[i];
            let var = ctx.builder.declare_var(cl_type(repr));
            ctx.builder.def_var(var, block_val);
            ctx.locals.insert(p.name.clone(), Local { var, repr });
        }

        // The implicit receiver `this` of a constructor/method: bind its class so
        // `this.field` resolves against the class shape and `this.method()`
        // dispatches statically.
        if let Some(class) = this_class {
            if let Some(desc) = classes.get(class) {
                let shape_id = ctx.shapes.intern(&desc.fields);
                ctx.local_shapes.insert(super::class::THIS.to_string(), HeapShape::Object(shape_id));
                ctx.local_classes.insert(super::class::THIS.to_string(), class.to_string());
            }
        }

        ctx.lower_block(module, &func.body)?;

        // A void main may fall off the end — emit the trailing `return`. A
        // value-returning function that can fall through is ill-formed; bail.
        if !ctx.block_terminated {
            match ctx.ret {
                None => {
                    ctx.builder.ins().return_(&[]);
                }
                Some(_) => {
                    return unsupported!(
                        "function `{}` may fall through without returning a value",
                        func.name
                    );
                }
            }
        }
        Ok(())
    }

    /// Lower a statement block (control flow is statement-driven; this does not
    /// open a Cranelift block). Stops once the block is terminated.
    pub(super) fn lower_block(
        &mut self,
        module: &mut dyn Module,
        stmts: &[HirStmt],
    ) -> FrontResult<()> {
        for s in stmts {
            if self.block_terminated {
                return unsupported!("unreachable statement after a terminator");
            }
            self.lower_stmt(module, s)?;
        }
        Ok(())
    }

    /// Look up a local binding by name.
    pub(super) fn local(&self, name: &str) -> Option<Local> {
        self.locals.get(name).copied()
    }

    // ---- coercions ----

    /// Coerce `val` to `target`, inserting box/unbox/widening as needed. The
    /// legal coercions:
    /// - numeric widening `Int* → Float64` (`fcvt_from_sint`);
    /// - the `Int32`/`Int64` relabel (same i64 register);
    /// - native → `Tagged` (BOX, pure IR);
    /// - `Tagged` → native number (UNBOX, pure IR) — used at a numeric call
    ///   boundary; the program lowering only does this when the target is proven
    ///   numeric.
    pub(super) fn coerce(&mut self, val: Val, target: Repr) -> FrontResult<Value> {
        if val.repr == target {
            return Ok(val.v);
        }
        match (val.repr, target) {
            // numeric widening to double
            (Repr::Int32, Repr::Float64) | (Repr::Int64, Repr::Float64) => {
                Ok(self.builder.ins().fcvt_from_sint(types::F64, val.v))
            }
            // int relabel (same register)
            (Repr::Int32, Repr::Int64) | (Repr::Int64, Repr::Int32) => Ok(val.v),
            // native → Tagged (box)
            (_, Repr::Tagged) => Ok(self.box_value(val)),
            // Tagged → native number (unbox)
            (Repr::Tagged, Repr::Float64) => {
                Ok(value::emit_unbox_double(self.builder, val.v))
            }
            (Repr::Tagged, Repr::Int32) | (Repr::Tagged, Repr::Int64) => {
                Ok(value::emit_unbox_int32(self.builder, val.v))
            }
            (from, to) => unsupported!("cannot coerce {from:?} to {to:?}"),
        }
    }

    /// BOX an unboxed value into a `Tagged` PolyValue word (pure IR). A `Bool`
    /// (i64 0/1) becomes the `false`/`true` singleton via `select`.
    pub(super) fn box_value(&mut self, val: Val) -> Value {
        match val.repr {
            Repr::Int32 => {
                let i32v = self.builder.ins().ireduce(types::I32, val.v);
                value::emit_box_int32(self.builder, i32v)
            }
            Repr::Int64 => {
                // A 64-bit int that must become Tagged boxes as a double (the
                // tagged int payload is only 48-bit; an i64 in JS-number range
                // round-trips exactly through f64 for the magnitudes here).
                let f = self.builder.ins().fcvt_from_sint(types::F64, val.v);
                value::emit_box_double(self.builder, f)
            }
            Repr::Float64 => value::emit_box_double(self.builder, val.v),
            Repr::Bool => {
                // false → SINGLETON_FALSE word, true → SINGLETON_TRUE word.
                let f_word = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::bool(false).raw() as i64);
                let t_word = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::bool(true).raw() as i64);
                self.builder.ins().select(val.v, t_word, f_word)
            }
            Repr::Tagged => val.v,
            other => {
                // Ref kinds are not produced by the current program subset.
                unreachable!("box_value on unexpected repr {other:?}")
            }
        }
    }

    // ---- extern calls (runtime ops) ----

    /// Declare-import a runtime symbol by name and emit the call, with each call's
    /// Cranelift signature derived EXACTLY from the real-symbol descriptor
    /// ([`crate::value::abi_sig`]) so `StrPtr` splits into ptr+len and f64/i64
    /// slots are right (mis-marshaling → SIGILL). `args` are the already-marshaled
    /// Cranelift values, one per Cranelift slot. Returns the result for returning
    /// symbols, or `None` for void ones.
    ///
    /// This serves the codegen-owned `__rtsadp_*` generic operators (all `U64`
    /// slots — PolyValue words in/out); the StrPtr-bearing real symbols are called
    /// through the dedicated [`crate::value::emit_marshal`] helpers.
    pub(super) fn call_runtime(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        args: &[Value],
    ) -> FrontResult<Option<Value>> {
        let sig = crate::value::abi_sig::sig_of(name)
            .ok_or_else(|| Unsupported::new(format!("unknown runtime symbol `{name}`")))?;
        if sig.param_slot_count() != args.len() {
            return unsupported!(
                "runtime symbol `{name}` expects {} slots but {} args",
                sig.param_slot_count(),
                args.len()
            );
        }
        Ok(crate::value::emit_marshal::emit_call(module, self.builder, name, args))
    }
}
