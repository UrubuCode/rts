//! Regex literal lowering + RegExp instance / string-regex-method dispatch (P5.12).
//!
//! `RegExp` is a RUNTIME/Registry class — the engine does NOT name it directly
//! (PRIMORDIAL-vs-Registry doctrine). The lowering here is a thin front-end over
//! the codegen-owned [`crate::value::regexops`] trampolines (which wrap the REAL
//! `__RTS_FN_NS_REGEX_*` symbols):
//!
//! - a regex LITERAL `/pat/flags` reaches the HIR as `Raw("regex\0<pat>\0<flags>")`
//!   (the `rts-hir` lowering encodes the swc `exp`/`flags` into the Raw payload —
//!   the MIR/old engine match `Raw(_)` and ignore the content, so it is
//!   backward-compatible). [`Lowerer::regex_literal_parts`] decodes it;
//!   [`Lowerer::lower_regex_literal`] interns the pattern + flags and emits the
//!   compile trampoline → a RegExp instance word (TAG_OBJECT).
//! - `new RegExp(pat[, flags])` lowers to the SAME compile path (in
//!   [`super::globalclass`]).
//! - `re.test(s)` / `.source` / `.flags` / `.global` dispatch on a recorded RegExp
//!   instance (also in `globalclass`'s metadata path).
//! - `s.match(re)` / `.replace` / `.replaceAll` / `.split` / `.search` dispatch
//!   here when the receiver is a proven string and the FIRST arg is a regex
//!   (a literal or a recorded RegExp local). The string-first `.replace("lit", r)`
//!   path (P5.2) is unaffected — only the regex-FIRST-arg case routes here.
//!
//! Bails (the honesty floor): a function replacer; capture-group `.exec`
//! extraction; a dynamic (non-literal, non-recorded) regex receiver.

use cranelift_codegen::ir::{InstBuilder, types};
use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::value::abi_adapter;

use crate::front::error::FrontResult;

use super::lower::{JsKind, Lowerer, Val};

/// The class name a recorded RegExp instance carries (for method dispatch).
pub(super) const REGEX_CLASS: &str = "RegExp";

/// Decode a regex-literal `Raw` payload (`"regex\0<pattern>\0<flags>"`, produced
/// by `rts-hir`'s `swc::Lit::Regex` lowering) into `(pattern, flags)`. Returns
/// `None` for any other `Raw` text (or a non-`Raw` node), so callers can probe an
/// arbitrary expr cheaply.
pub(super) fn regex_literal_parts(expr: &HirExpr) -> Option<(String, String)> {
    let HirExprKind::Raw(text) = &expr.kind else {
        return None;
    };
    let rest = text.strip_prefix("regex\0")?;
    // `pattern\0flags` — a regex source never contains a NUL, so the first NUL is
    // the separator (and there is exactly one). `flags` may be empty.
    let mut parts = rest.splitn(2, '\0');
    let pattern = parts.next()?.to_string();
    let flags = parts.next().unwrap_or("").to_string();
    Some((pattern, flags))
}

/// Whether `expr` is a regex literal (`/pat/flags`).
pub(super) fn is_regex_literal(expr: &HirExpr) -> bool {
    regex_literal_parts(expr).is_some()
}

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Lower a regex literal `/pat/flags` to a RegExp instance word: intern the
    /// pattern + flags in the REAL string pool and emit the compile trampoline.
    /// The result is a `TAG_OBJECT` word tagged kind `Object` (the RegExp instance
    /// representation); the caller records the static class for method dispatch.
    pub(super) fn lower_regex_literal(
        &mut self,
        module: &mut dyn Module,
        pattern: &str,
        flags: &str,
    ) -> FrontResult<Val> {
        let pat_pv = abi_adapter::intern_poly(pattern);
        let flags_pv = abi_adapter::intern_poly(flags);
        let pat_w = self.builder.ins().iconst(types::I64, pat_pv.raw() as i64);
        let flags_w = self.builder.ins().iconst(types::I64, flags_pv.raw() as i64);
        let word = self
            .call_runtime(module, "__rtsadp_re_compile", &[pat_w, flags_w])?
            .expect("regex compile returns a value");
        Ok(Val::tagged_kind(word, JsKind::Object))
    }

    /// Whether `expr` resolves to a RegExp instance the engine can dispatch on:
    /// a regex LITERAL, a `new RegExp(..)` directly, or a bare identifier recorded
    /// as a RegExp local (`global_instance_classes`).
    pub(super) fn is_regex_value(&self, expr: &HirExpr) -> bool {
        if is_regex_literal(expr) {
            return true;
        }
        match &expr.kind {
            HirExprKind::New { class, .. } => class == REGEX_CLASS,
            HirExprKind::Ident(name) => {
                self.global_instance_classes.get(name).map(String::as_str) == Some(REGEX_CLASS)
            }
            _ => false,
        }
    }

    /// Lower an expression KNOWN (via [`Self::is_regex_value`]) to be a RegExp
    /// instance to its instance word. A literal compiles fresh; a recorded local /
    /// `new RegExp(..)` lowers through the normal expr path (which yields its
    /// TAG_OBJECT word).
    pub(super) fn lower_regex_value(
        &mut self,
        module: &mut dyn Module,
        expr: &HirExpr,
    ) -> FrontResult<Val> {
        if let Some((pattern, flags)) = regex_literal_parts(expr) {
            return self.lower_regex_literal(module, &pattern, &flags);
        }
        self.lower_expr(module, expr)
    }

    /// Try to lower a STRING-with-regex method `s.method(re[, repl])` where `s` is a
    /// proven string and the FIRST arg is a regex value. Returns `Ok(Some(val))` on
    /// a handled `match`/`replace`/`replaceAll`/`split`/`search`, `Ok(None)` when
    /// `method` is not one of these OR the first arg is not a regex (so the caller
    /// falls through to the string-first-arg path / generic row), or a bail for a
    /// divergent form (e.g. a function replacer).
    pub(super) fn try_string_regex_method(
        &mut self,
        module: &mut dyn Module,
        recv: Val,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        // Only string receivers with a regex first arg route here.
        if args.is_empty() || !self.is_regex_value(&args[0]) {
            return Ok(None);
        }
        let recv_word = self.box_value(recv);

        match (method, args.len()) {
            // `s.match(re)` → array of match strings, or `null`.
            ("match", 1) => {
                let re = self.lower_regex_value(module, &args[0])?;
                let re_word = self.box_value(re);
                let word = self
                    .call_runtime(module, "__rtsadp_re_str_match", &[recv_word, re_word])?
                    .expect("re_str_match returns a value");
                // The result is an array of match strings (or `null` on no match).
                // Tag it `Array` so a `let m = s.match(re)` records the array shape
                // and `m[0]` / `m.length` resolve. CAVEAT (JS-faithful): on no match
                // the value is `null`; `m[0]` then dereferences null — the same
                // throw JS produces (we do not paper over it with a wrong value).
                Ok(Some(Val::new(word, crate::repr::Repr::Tagged)))
            }
            // `s.search(re)` → index (number) or -1.
            ("search", 1) => {
                let re = self.lower_regex_value(module, &args[0])?;
                let re_word = self.box_value(re);
                let word = self
                    .call_runtime(module, "__rtsadp_re_str_search", &[recv_word, re_word])?
                    .expect("re_str_search returns a value");
                Ok(Some(Val::tagged_kind(word, JsKind::Number)))
            }
            // `s.split(re, limit?)` → the FULL JS-spec regex split (capture
            // groups spliced into the result, ToUint32 limit, per-position
            // empty-match rule) — one polymorphic trampoline; the omitted limit
            // rides `undefined`.
            ("split", 1 | 2) => {
                let re = self.lower_regex_value(module, &args[0])?;
                let re_word = self.box_value(re);
                let limit_word = match args.get(1) {
                    Some(a) => {
                        let v = self.lower_expr(module, a)?;
                        self.box_value(v)
                    }
                    None => self.builder.ins().iconst(
                        cranelift_codegen::ir::types::I64,
                        crate::value::PolyValue::undefined().raw() as i64,
                    ),
                };
                let word = self
                    .call_runtime(
                        module,
                        "__rtsadp_re_str_split",
                        &[recv_word, re_word, limit_word],
                    )?
                    .expect("re_str_split returns a value");
                Ok(Some(Val::tagged_kind(word, JsKind::Array)))
            }
            // `s.replace(re, repl)` / `s.replaceAll(re, repl)` → string, via the
            // ONE polymorphic trampoline: a STRING replacement runs the JS
            // `GetSubstitution` token expansion ($$, $&, $`, $', $n, $<name>);
            // a FUNCTION word is invoked per match (match, caps…, offset,
            // string); any other value ToStrings — tag-dispatched at runtime,
            // never a compile-time guess. A global regex with `.replace`
            // replaces all (JS).
            ("replace" | "replaceAll", 2) => {
                let global = self.regex_is_global(&args[0]);
                let repl = self.lower_expr(module, &args[1])?;
                let re = self.lower_regex_value(module, &args[0])?;
                let re_word = self.box_value(re);
                let repl_word = self.box_value(repl);
                let all = i64::from(method == "replaceAll" || global);
                let all_v = self
                    .builder
                    .ins()
                    .iconst(cranelift_codegen::ir::types::I64, all);
                let word = self
                    .call_runtime(
                        module,
                        "__rtsadp_re_str_replace_fn",
                        &[recv_word, re_word, repl_word, all_v],
                    )?
                    .expect("re_str_replace_fn returns a value");
                Ok(Some(Val::tagged_kind(word, JsKind::Str)))
            }
            _ => Ok(None),
        }
    }

    /// Whether a regex value statically has the `g` flag — known only for a regex
    /// LITERAL (the flags are in the HIR payload). A `new RegExp(..)` / recorded
    /// local's flags are not statically known here, so it is treated as
    /// non-global; `s.replace(reLocal, x)` with a global local would only replace
    /// the first match (a known, documented limit — JS-vs-static).
    fn regex_is_global(&self, expr: &HirExpr) -> bool {
        regex_literal_parts(expr)
            .map(|(_, flags)| flags.contains('g'))
            .unwrap_or(false)
    }
}
