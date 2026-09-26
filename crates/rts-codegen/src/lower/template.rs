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
//! # One join where a site was minted
//!
//! A template of one to three substitutions whose door minted it a site is joined by
//! `TemplateJoin`, in ONE crossing that allocates once, where the chain allocates a
//! string per `+` and makes each one garbage at the next. The conversions stay here,
//! each after its own substitution, because that is the order the language evaluates
//! them in -- `ToString` over an object runs its `toString`, and a later substitution
//! may read what it did. `optimize::fuse_templates` is what removes a conversion, once
//! the inference has proved it could not run anything.
//!
//! Three because the entry's arguments are scalars across an `extern "C"` boundary, as
//! `emit/template.rs` says of the same call. A wider template keeps the chain.

use rts_mir::cfg::ValueId;

use super::{Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim};
use crate::runtime::RuntimeOp;
use crate::syntax::{Expr, TemplatePart};

/// How many substitutions `TemplateJoin` takes.
pub(crate) const JOINED: usize = 3;

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
        if let Some(site) = self.callees.template_site(at.at)
            && (1..=JOINED).contains(&expressions.len())
        {
            return self.joined(site, expressions, at);
        }
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

    /// The pieces from the site and each substitution converted, in one crossing.
    fn joined(&mut self, site: u32, expressions: &[Expr], at: &Expr) -> Result<ValueId, Unsupported> {
        let site = self.domain.constant(JsConst::Count(site));
        let count = self.domain.constant(JsConst::Count(expressions.len() as u32));
        let mut args = vec![self.declared(site, at), self.declared(count, at)];
        for value in expressions {
            let value = self.expression(value)?;
            args.push(self.entry(RuntimeOp::StringOf, vec![value], at));
        }
        // The slots past `count` are never read; `undefined` fills them because an
        // argument the entry declares has to be something.
        while args.len() < 2 + JOINED {
            args.push(self.singleton_at(crate::values::Singleton::Undefined, at));
        }
        Ok(self.entry(RuntimeOp::TemplateJoin, args, at))
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
