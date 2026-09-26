//! `` `a${b}c` ``: the first text, then each substitution converted to a string and
//! joined, then the text after it.
//!
//! # Why the conversion is a call and not the `+` that follows it
//!
//! `+` converts an object with the DEFAULT hint, which asks `valueOf` first; a template
//! substitution converts with the STRING hint, which asks `toString` first. So
//! `` `${{ toString: () => "T", valueOf: () => 42 }}` `` is `"T"` and `"" + that` is
//! `"42"`, and no spelling of `+` repairs it -- `emit/template.rs` found that the
//! operator's definition is the wrong one here. `RuntimeOp::StringOf` is ToString, the
//! call the running engine makes for the same reason, and once each operand is a
//! string, `+` over two strings is exactly concatenation.
//!
//! The running engine also has `TemplateJoin`, one call per template over a site of
//! declared pieces. It is a speed shape over this one rather than a different meaning,
//! and this is the shape `emit/template.rs` itself falls back to -- so the graph says
//! what a template means and a pass may later say it faster.

use rts_mir::cfg::ValueId;

use super::{Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim};
use crate::runtime::RuntimeOp;
use crate::syntax::{Expr, TemplatePart};

impl Lowering<'_> {
    /// An untagged template literal.
    pub(super) fn template(
        &mut self,
        parts: &[TemplatePart],
        expressions: &[Expr],
        at: &Expr,
    ) -> Result<ValueId, Unsupported> {
        let Some((first, rest)) = parts.split_first() else {
            return Err(Unsupported::Expression(
                "a template with no text piece, which the tree says cannot exist",
            ));
        };
        let mut joined = self.text_piece(first, at)?;
        for (value, part) in expressions.iter().zip(rest) {
            let value = self.expression(value)?;
            let text = self.entry(RuntimeOp::StringOf, vec![value], at);
            joined = self.prim(JsPrim::Add, vec![joined, text], at);
            if part
                .cooked
                .as_ref()
                .is_some_and(|held| held.units().is_empty())
            {
                continue;
            }
            let piece = self.text_piece(part, at)?;
            joined = self.prim(JsPrim::Add, vec![joined, piece], at);
        }
        Ok(joined)
    }

    /// One literal piece, as a text constant.
    ///
    /// An untagged template whose escape is invalid is a syntax error, so a piece with
    /// no cooked text is refused rather than read as `undefined` -- which is what a
    /// TAGGED template hands its tag, and the difference is the language's.
    fn text_piece(&mut self, part: &TemplatePart, at: &Expr) -> Result<ValueId, Unsupported> {
        let Some(cooked) = &part.cooked else {
            return Err(Unsupported::Expression(
                "an untagged template with an invalid escape, which is a syntax error",
            ));
        };
        let index = self.domain.constant(JsConst::Text(cooked.clone()));
        Ok(self.declared(index, at))
    }
}
