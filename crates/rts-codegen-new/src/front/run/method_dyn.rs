//! DYNAMIC (runtime) method-dispatch lowering — P5.9.
//!
//! When `recv.method(args)` has a method name known at compile time but a receiver
//! whose CLASS is only known at runtime (a Tagged value of `Unknown` kind: a
//! param, a call return, a re-`let` local), the static dispatch
//! ([`super::method`]) returns `Ok(None)` and the caller would bail
//! ("method call on non-dispatchable receiver"). This module is the fallback: it
//! lowers such a call to ONE `call` of a codegen-owned `__rtsadp_dyn_<method>`
//! trampoline (see [`crate::value::dyndispatch`]) that inspects the receiver's
//! PolyValue tag at RUNTIME and delegates to the correct per-class op.
//!
//! ## Marshaling — uniform PolyValue words
//!
//! The trampolines use a uniform `U64` PolyValue-word ABI (receiver word + 0..2
//! arg words → one result word), so the lowering simply BOXES the receiver and
//! each arg to a word and UNBOXES the single result word. The result's static kind
//! is recorded per method ([`DynMethod::ret_kind`]) so a chained call / `let`
//! shape works (e.g. `.split(..)` yields an Array, `.toUpperCase()` a string).
//!
//! ## Soundness
//!
//! - Only a method present in [`DYN_METHODS`] is dynamically dispatched, and only
//!   when EVERY runtime tag the trampoline handles yields the JS-correct value;
//!   an unexpected tag returns `undefined` (a TypeError class in JS, never a wrong
//!   value — the honesty floor).
//! - The STATIC fast path is unchanged: this is reached ONLY after every
//!   static-class check has declined (the receiver is genuinely Tagged/Unknown).
//! - A callback argument (arrow / function-expression) is NOT dynamically
//!   dispatched here (array callbacks need a proven array receiver + reification) —
//!   such a call BAILS at the caller, never guesses.

use cranelift_codegen::ir::{types, InstBuilder, Value};
use cranelift_module::Module;

use rts_hir::HirExpr;

use crate::repr::Repr;

use crate::front::error::FrontResult;

use super::lower::{JsKind, Lowerer, Val};

/// One dynamically-dispatchable method: its JS name, explicit (non-receiver)
/// arity, the `__rtsadp_dyn_*` trampoline symbol, and the static kind of the
/// result word (so a `let`/chain on the result keeps the right shape).
struct DynMethod {
    /// The JS method name as written (`"toUpperCase"`, `"indexOf"`, …).
    name: &'static str,
    /// The number of EXPLICIT (after-receiver) arguments the user call must have.
    argc: usize,
    /// The codegen-owned trampoline symbol the lowering `call`s.
    symbol: &'static str,
    /// The static kind of the returned PolyValue word — drives `Val` kind so a
    /// chained `.method()` / `let x = ...` records the right shape.
    ret_kind: JsKind,
}

/// The dynamically-dispatchable method table. HIGH-FREQUENCY methods first.
/// Each row's trampoline handles every receiver tag soundly (per-class op or the
/// `undefined` sentinel). A method present on MULTIPLE classes (e.g. `indexOf` on
/// both string and array) appears once — the trampoline branches internally.
///
/// `ret_kind`:
/// - `Str` — a string result (`toString`/`slice`-on-string/`charAt`/`split`-elem…)
///   so `console.log` prints it bare and a chained string method works;
/// - `Array` — a fresh array (`split`/`slice`-on-array/`concat`-on-array) so a
///   `let` records `HeapShape::Array`;
/// - `Number`/`Bool` — a number/bool result (`length`/`indexOf`/`includes`/…);
/// - `Unknown` — an opaque element word (`at`/`pop`) whose kind is not known
///   statically (it could be anything stored in the array).
const DYN_METHODS: &[DynMethod] = &[
    // ---- defined on every value / both string+array ----
    DynMethod { name: "toString", argc: 0, symbol: "__rtsadp_dyn_to_string", ret_kind: JsKind::Str },
    DynMethod { name: "indexOf", argc: 1, symbol: "__rtsadp_dyn_index_of", ret_kind: JsKind::Number },
    DynMethod { name: "includes", argc: 1, symbol: "__rtsadp_dyn_includes", ret_kind: JsKind::Bool },
    DynMethod { name: "at", argc: 1, symbol: "__rtsadp_dyn_at", ret_kind: JsKind::Unknown },
    DynMethod { name: "concat", argc: 1, symbol: "__rtsadp_dyn_concat", ret_kind: JsKind::Unknown },
    // ---- array-only ----
    DynMethod { name: "join", argc: 1, symbol: "__rtsadp_dyn_join", ret_kind: JsKind::Str },
    DynMethod { name: "push", argc: 1, symbol: "__rtsadp_dyn_push", ret_kind: JsKind::Number },
    DynMethod { name: "pop", argc: 0, symbol: "__rtsadp_dyn_pop", ret_kind: JsKind::Unknown },
    // ---- string-only ----
    DynMethod { name: "toUpperCase", argc: 0, symbol: "__rtsadp_dyn_to_upper_case", ret_kind: JsKind::Str },
    DynMethod { name: "toLowerCase", argc: 0, symbol: "__rtsadp_dyn_to_lower_case", ret_kind: JsKind::Str },
    DynMethod { name: "trim", argc: 0, symbol: "__rtsadp_dyn_trim", ret_kind: JsKind::Str },
    DynMethod { name: "charAt", argc: 1, symbol: "__rtsadp_dyn_char_at", ret_kind: JsKind::Str },
    DynMethod { name: "charCodeAt", argc: 1, symbol: "__rtsadp_dyn_char_code_at", ret_kind: JsKind::Number },
    DynMethod { name: "split", argc: 1, symbol: "__rtsadp_dyn_split", ret_kind: JsKind::Array },
    DynMethod { name: "startsWith", argc: 1, symbol: "__rtsadp_dyn_starts_with", ret_kind: JsKind::Bool },
    DynMethod { name: "endsWith", argc: 1, symbol: "__rtsadp_dyn_ends_with", ret_kind: JsKind::Bool },
    DynMethod { name: "repeat", argc: 1, symbol: "__rtsadp_dyn_repeat", ret_kind: JsKind::Str },
];

/// `.slice` is special: it has BOTH a 1-arg ("to end") and a 2-arg form, and its
/// result kind depends on the receiver (string→string, array→array). The lowering
/// supplies the defaulted end bound for the 1-arg form and records the result as
/// `Unknown` (it could be either) so neither a wrong string nor array shape is
/// asserted. Handled in [`Lowerer::try_method_dispatch_dynamic`] directly.
const SLICE_SYMBOL: &str = "__rtsadp_dyn_slice";

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Try to lower `recv.method(args)` via RUNTIME tag dispatch. `recv` is the
    /// ALREADY-lowered receiver `Val` (a Tagged value of unproven class). Returns
    /// `Ok(Some(val))` when `method` is dynamically dispatchable at the given
    /// arity, `Ok(None)` when it is not in the table (the caller bails) or a
    /// callback arg is present (array callbacks need a proven receiver).
    ///
    /// Only reached AFTER every static-class check declined, so the static fast
    /// path is never perturbed.
    pub(super) fn try_method_dispatch_dynamic(
        &mut self,
        module: &mut dyn Module,
        recv: Val,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        // A callback argument is never dynamically dispatched here (an array
        // callback method needs a PROVEN array receiver + callback reification —
        // a Tagged receiver cannot supply that). Decline → the caller bails.
        if args.iter().any(is_callback_arg) {
            return Ok(None);
        }

        // `.slice` (string or array): inject the defaulted "to end" bound for the
        // 1-arg form, then call the 3-word trampoline. Result kind Unknown (could
        // be a string or an array depending on the runtime receiver).
        if method == "slice" && (args.len() == 1 || args.len() == 2) {
            return self.emit_dyn_slice(module, recv, args).map(Some);
        }

        let Some(m) = DYN_METHODS.iter().find(|m| m.name == method && m.argc == args.len()) else {
            return Ok(None);
        };

        // Box the receiver + each arg to a raw PolyValue word.
        let recv_word = self.box_value(recv);
        let mut call_args: Vec<Value> = Vec::with_capacity(args.len() + 1);
        call_args.push(recv_word);
        for a in args {
            let v = self.lower_expr(module, a)?;
            call_args.push(self.box_value(v));
        }

        let word = self
            .call_runtime(module, m.symbol, &call_args)?
            .expect("a dynamic-dispatch trampoline returns a value");
        Ok(Some(self.tag_dyn_result(word, m.ret_kind)))
    }

    /// Lower the dynamic `.slice(start[, end])`: box the receiver + start, supply
    /// the defaulted `i64::MAX` "to end" bound (boxed) for the 1-arg form, then call
    /// `__rtsadp_dyn_slice`. The result kind is `Unknown` (string or array).
    fn emit_dyn_slice(
        &mut self,
        module: &mut dyn Module,
        recv: Val,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let recv_word = self.box_value(recv);
        let start = self.lower_expr(module, &args[0])?;
        let start_word = self.box_value(start);
        let end_word = if args.len() == 2 {
            let end = self.lower_expr(module, &args[1])?;
            self.box_value(end)
        } else {
            // The "to end" default: a large int the trampolines clamp to length.
            let pv = crate::value::PolyValue::from_i32(i32::MAX);
            self.builder.ins().iconst(types::I64, pv.raw() as i64)
        };
        let word = self
            .call_runtime(module, SLICE_SYMBOL, &[recv_word, start_word, end_word])?
            .expect("__rtsadp_dyn_slice returns a value");
        Ok(self.tag_dyn_result(word, JsKind::Unknown))
    }

    /// Wrap a dynamic-dispatch result word as a `Val`. A `Number`/`Bool` result is
    /// still a Tagged word here (the trampoline boxes it), so it rides `Tagged`
    /// with the proven kind — the `console.log`/`+` paths re-read the tag. Only the
    /// kind hint differs, which drives chaining + inspect.
    fn tag_dyn_result(&self, word: Value, kind: JsKind) -> Val {
        Val::new_with_kind(word, Repr::Tagged, kind)
    }
}

/// Whether an argument expression is a callback (a function/arrow value) — such a
/// method is not dynamically dispatched (it needs a proven array receiver).
fn is_callback_arg(e: &HirExpr) -> bool {
    matches!(&e.kind, rts_hir::ir::HirExprKind::Arrow { .. })
}
