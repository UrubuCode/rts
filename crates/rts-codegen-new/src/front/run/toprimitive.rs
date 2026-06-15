//! Lowering-time JS `ToPrimitive` for STATICALLY-KNOWN-CLASS objects (issue #304).
//!
//! When an OBJECT value flows into a STRING/NUMBER coercion context — a template
//! interpolation `${obj}`, the string-coercing `+` (`obj + ""`), `String(obj)` —
//! JS runs the object's `Symbol.toPrimitive`/`valueOf`/`toString` chain. That can
//! only be honored when the object's CLASS is statically known at lowering time
//! (a `new C()` instance, a `C`-recorded local, `this`), because the method to
//! call is a JIT'd user method — the runtime `js_to_string` trampoline (Rust)
//! cannot dispatch a JIT'd method, so it would render the default `[object
//! Object]` and diverge from Node/Bun.
//!
//! So this module resolves ToPrimitive HERE, at the call site, where the receiver
//! class is in scope:
//!
//! - a USER class whose chain defines `toString`/`valueOf` → emit a direct call of
//!   that synthesized method (`this` = the instance), then ToString the result;
//! - a runtime `Error` instance → `Error.prototype.toString` (`"name: message"`);
//! - everything else (plain object, array, Map/Set, a DYNAMIC/unknown-class
//!   receiver) → `Ok(None)`: the caller keeps its existing default behavior
//!   (`array.join(",")`, `[object Object]`, or an explicit bail).
//!
//! ## Hint
//! `Hint::String` (template / `String()` / a string-context `+`) tries `toString`
//! first; `Hint::Default` (the bare `+` operator) tries `valueOf` first. This
//! mirrors the spec's `OrdinaryToPrimitive` ordering. `Symbol.toPrimitive` is not
//! modeled (the engine carries zero Symbol mentions — design doctrine); an object
//! defining only it falls through to the default and ultimately bails.
//!
//! ## console.log is NOT a ToPrimitive context
//! `console.log(obj)` uses `util.inspect`, which does NOT call `toString` — it
//! renders the object's own shape (`C {}` / `{ a: 1 }`). So `lower_console_log`
//! does NOT route through here; only `String()`/template/`+` do. Matching JS
//! exactly is the point of keeping these paths separate.

use cranelift_codegen::ir::Value;
use cranelift_module::Module;

use rts_hir::HirExpr;

use crate::front::error::FrontResult;

use super::lower::Lowerer;

/// The coercion hint, deciding the `valueOf`/`toString` try order.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Hint {
    /// A string context (template `${}`, `String()`, a string-seeded `+`): try
    /// `toString` first, then `valueOf`.
    String,
    /// The default context (the bare `+` operator with no proven string side):
    /// try `valueOf` first, then `toString`.
    Default,
}

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// JS `ToString(ToPrimitive(expr, hint))` for a STATICALLY-KNOWN-CLASS object,
    /// emitted as a direct call of the object's `toString`/`valueOf` method. Returns
    /// `Ok(Some(word))` (a STRING PolyValue word) when `expr` is such an object and
    /// the engine can run its primitive chain; `Ok(None)` when `expr` is not a
    /// known-class object with a usable method (the caller keeps its default).
    ///
    /// The chosen method's result is ToString'd through the one generic
    /// `__rtsadp_to_string` path, so a `valueOf` returning a number formats exactly
    /// like the rest of the runtime.
    pub(super) fn coerce_object_to_string_word(
        &mut self,
        module: &mut dyn Module,
        expr: &HirExpr,
        hint: Hint,
    ) -> FrontResult<Option<Value>> {
        let Some(prim) = self.coerce_object_to_primitive_word(module, expr, hint)? else {
            return Ok(None);
        };
        // ToString the primitive (a string stays itself; a number/bool formats via
        // the runtime's own Number→String, byte-identical to everything else).
        let s = self
            .call_runtime(module, "__rtsadp_to_string", &[prim])?
            .expect("__rtsadp_to_string returns a value");
        Ok(Some(s))
    }

    /// JS `ToPrimitive(expr, hint)` for a STATICALLY-KNOWN-CLASS object, emitted as
    /// a direct call of the resolved `toString`/`valueOf` method — returning the
    /// RAW primitive PolyValue word (a number from `valueOf`, a string from
    /// `toString`). Returns `Ok(None)` when `expr` is not a known-class object with
    /// a usable method (the caller keeps its default `[object Object]` / array-join
    /// / bail). Used by the `+` operator (which then re-dispatches on the resulting
    /// primitive tags) and as the building block of [`Self::coerce_object_to_string_word`].
    pub(super) fn coerce_object_to_primitive_word(
        &mut self,
        module: &mut dyn Module,
        expr: &HirExpr,
        hint: Hint,
    ) -> FrontResult<Option<Value>> {
        // ---- USER class instance (the high-value case) ----
        if let Some(class) = self.static_instance_class(expr) {
            if let Some(method) = self.primitive_method_of(&class, hint) {
                let val = self.try_class_method(module, expr, &class, &method, &[])?;
                return Ok(Some(self.box_value(val)));
            }
            // A known user class with NEITHER toString nor valueOf → the default
            // `[object Object]` is correct; let the caller's existing path render it.
            return Ok(None);
        }

        // NOTE: an `.ts` Error instance is a USER class (prelude `Error.ts`), so its
        // `toString()` (`"name: message"`) is resolved by the user-class branch
        // above — no `__rtsadp_err_to_string` trampoline. A thrown-then-caught error
        // (opaque `e`) has no static class here, so `${e}`/`String(e)` on it falls to
        // the caller's default; the common `e.message`/`e.toString()` forms read the
        // shape slots / call the `.ts` method directly.

        // Everything else (plain object, array, Map/Set, dynamic receiver) → the
        // caller's default. NOT a guess: arrays join via the runtime `js_to_string`,
        // plain objects render `[object Object]`, Map/Set bail upstream.
        Ok(None)
    }

    /// The synthesized method name to call for `ToPrimitive` on `class` under
    /// `hint`, walking the class chain: `toString`-first for a string hint,
    /// `valueOf`-first for the default hint, falling back to the other. `None` when
    /// the class (and its ancestors) define NEITHER.
    fn primitive_method_of(&self, class: &str, hint: Hint) -> Option<String> {
        let desc = self.classes.get(class)?;
        let has_to_string = desc.method_fn("toString").is_some();
        let has_value_of = desc.method_fn("valueOf").is_some();
        let pick = match hint {
            Hint::String => {
                if has_to_string {
                    "toString"
                } else {
                    // STRING hint with NO own `toString`: JS uses the INHERITED
                    // `Object.prototype.toString` (`"[object Object]"`), NOT `valueOf`
                    // (whose result the string-hint `OrdinaryToPrimitive` never reaches
                    // when `toString` exists on the prototype). The engine does not
                    // model the inherited method, so it resolves NO own primitive
                    // method here → `None`, and the caller falls to its default/bail
                    // (never the WRONG `valueOf` string). A faithful `[object Object]`
                    // for this case is a later increment.
                    return None;
                }
            }
            Hint::Default => {
                if has_value_of {
                    "valueOf"
                } else if has_to_string {
                    "toString"
                } else {
                    return None;
                }
            }
        };
        Some(pick.to_string())
    }

    /// Whether `expr` is a known-class object for which the engine can run a
    /// lowering-time `ToPrimitive` (a user class with `toString`/`valueOf` — incl.
    /// the prelude `.ts` Error family). Used by the `+` operator to decide whether
    /// to take the ToPrimitive path instead of the whole-object bail.
    pub(super) fn has_object_toprimitive(&self, expr: &HirExpr) -> bool {
        if let Some(class) = self.static_instance_class(expr) {
            return self.primitive_method_of(&class, Hint::Default).is_some();
        }
        false
    }

    /// Lower a `+` where at least one operand is a known-class object with a usable
    /// `ToPrimitive` (issue #304). Each operand is reduced to a PolyValue word: an
    /// object-with-primitive operand via its `valueOf`/`toString` (JS `+` uses the
    /// "default" hint); any other operand by ordinary lowering + box. The two words
    /// then go through the ONE generic `__rtsadp_add` path, which re-dispatches on
    /// the resulting primitive tags (number+number → add, string anywhere → concat)
    /// — exactly JS `+` after ToPrimitive.
    pub(super) fn lower_add_with_toprimitive(
        &mut self,
        module: &mut dyn Module,
        lhs: &HirExpr,
        rhs: &HirExpr,
    ) -> FrontResult<super::lower::Val> {
        let a = self.add_operand_word(module, lhs)?;
        let b = self.add_operand_word(module, rhs)?;
        let res = self
            .call_runtime(module, "__rtsadp_add", &[a, b])?
            .expect("__rtsadp_add returns a value");
        Ok(super::lower::Val::new(res, crate::repr::Repr::Tagged))
    }

    /// Reduce one `+` operand to a PolyValue word, applying `ToPrimitive` (default
    /// hint) when the operand is a known-class object; otherwise ordinary lower+box.
    fn add_operand_word(&mut self, module: &mut dyn Module, expr: &HirExpr) -> FrontResult<Value> {
        if let Some(word) = self.coerce_object_to_primitive_word(module, expr, Hint::Default)? {
            return Ok(word);
        }
        let v = self.lower_expr(module, expr)?;
        Ok(self.box_value(v))
    }
}
