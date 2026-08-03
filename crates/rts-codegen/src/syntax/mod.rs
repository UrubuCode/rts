//! The tree a program is written in.
//!
//! Shaped for what has to be decided rather than for what was typed. `a.b` and
//! `a[e]` are different nodes because one has a key and the other has an
//! expression that must be evaluated first, and a tree that made them the same
//! would push that difference into every place that reads one. The same reason
//! separates `new` from a call, `&&` from `+`, and a hole in an array from an
//! `undefined` in one.
//!
//! What the tree deliberately does *not* do is normalise. `a += b` is kept as a
//! compound assignment rather than rewritten to `a = a + b`, because the rewrite
//! is wrong — it evaluates the target twice. A tree that desugars is a tree that
//! has already made semantic decisions somewhere no test is looking.

mod expr;
mod stmt;

pub use expr::{BinaryOp, Expr, ExprKind, Literal, LogicalOp, Property, PropertyKey, UnaryOp};
pub use stmt::{Binding, BindingKind, Catch, Function, Parameter, Program, Stmt, StmtKind};

/// What a program said about a value.
///
/// A claim, not a type. TypeScript's annotations are erased before anything runs
/// and are unchecked at every boundary the program does not own — an argument
/// from JavaScript, a parsed document, a foreign call. So an annotation is
/// evidence about what a value probably is, and the question of what it is worth
/// belongs to whatever weighs claims, not to the tree that carries one.
///
/// This is deliberately a small vocabulary. It names the distinctions a
/// representation decision can act on and nothing else: a claim this crate cannot
/// turn into a representation is a claim it has no use for, and modelling
/// TypeScript's whole type language here would be modelling it twice.
#[derive(Clone, PartialEq, Debug)]
pub enum Claim {
    /// `number`.
    Number,
    /// `string`.
    Str,
    /// `boolean`.
    Boolean,
    /// `undefined`.
    Undefined,
    /// `null`.
    Null,
    /// An object of a named kind.
    ///
    /// The name is carried rather than resolved, because resolving it needs the
    /// program's declarations, which the tree does not have and should not.
    Object(crate::names::Name),
    /// An array of something.
    Array(Box<Claim>),
    /// One of several.
    ///
    /// Kept as written rather than reduced, because the reduction depends on
    /// facts a tree does not hold — `A | A` collapses, `A | B` does not, and
    /// deciding which is which needs the declarations.
    Union(Vec<Claim>),
    /// `any`, `unknown`, or anything this vocabulary does not name.
    ///
    /// Not a failure to parse: it is the honest answer for a claim that proves
    /// nothing, and collapsing all of those into one is what stops a later pass
    /// from mistaking an unhandled annotation for a useful one.
    Unknown,
}

impl Claim {
    /// Whether this claim, if true, names exactly one kind of value.
    ///
    /// The only question the representation decision asks of a claim. A union
    /// answers no even when every member is a number, because a claim that has
    /// to be examined before it answers is a claim that did not answer.
    pub fn is_definite(&self) -> bool {
        matches!(
            self,
            Claim::Number
                | Claim::Str
                | Claim::Boolean
                | Claim::Undefined
                | Claim::Null
                | Claim::Object(_)
                | Claim::Array(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_is_not_definite_and_neither_is_a_union() {
        assert!(Claim::Number.is_definite());
        assert!(!Claim::Unknown.is_definite());
        assert!(
            !Claim::Union(vec![Claim::Number, Claim::Number]).is_definite(),
            "a claim that must be examined before it answers has not answered"
        );
    }
}
