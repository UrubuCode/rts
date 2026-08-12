//! What a class body's members declare, beyond their names.
//!
//! # Why this is a module and not four more lines in `item.rs`
//!
//! Because `item.rs` is over the ceiling — 1307 lines against 1000 — and the
//! rule is that new code lands in a small focused module rather than being
//! appended to something already oversized. Four lines is exactly the size at
//! which that rule is tempting to skip, and skipping it is how a file gets from
//! 1000 to 1307.
//!
//! # What a field's annotation is for
//!
//! `class Point { x: number }` says two things: that instances have an `x`, and
//! that it holds a number. The tree kept the first and dropped the second — the
//! two property arms built a `Field` without ever reading `type_ann` — so a
//! layout pass asking what a class's instances look like could learn the shape
//! and not the representations.
//!
//! It is carried and not resolved, which is the same discipline `Claim` states
//! for every other annotation: this is evidence a program offered about a value,
//! unchecked by anything, and what it is worth belongs to whatever weighs it.
//!
//! # Two members that carry no claim, deliberately
//!
//! A method and a static block. A method's annotation is its RETURN type, which
//! `Function::returns` already carries, and a static block declares nothing.

use swc_ecma_ast as swc;

use super::Cx;
use super::item::claim;
use crate::syntax::Claim;

/// What a class property was annotated with, if it was annotated.
///
/// Takes the annotation rather than the property so the two arms — a public
/// property and a private one, which are different swc nodes holding the same
/// field — reach one answer instead of two. They already differed once: the
/// private arm is where a `#x` becomes a name, and a second reading of the
/// annotation beside it is exactly where the two would come to disagree about
/// what `#x: number` claims.
pub(super) fn field_claim(
    cx: &mut Cx,
    annotation: Option<&swc::TsTypeAnn>,
) -> Option<Claim> {
    Some(claim(cx, &annotation?.type_ann))
}
