//! What a refusal is called, and which primitive an operator is.
//!
//! Apart from the lowering because `mod.rs` passed 1000 lines against this crate's
//! ceiling, and this is the cohesive half: every function here answers a question
//! ABOUT the tree rather than lowering any of it, and none of them touches the
//! builder.
//!
//! The naming matters more than its size suggests. `Unsupported` carries one variant
//! per reason because the survey over a corpus IS the work queue, and a refusal that
//! does not name itself cannot be queued -- a fall-through arm here was once the
//! biggest single row in `bench/`.

use crate::domain::JsPrim;
use crate::syntax::{BinaryOp, ExprKind, StmtKind};
/// The primitive an operator is, or `None` where this language's table has no row
/// for it yet.
pub(super) fn primitive(op: BinaryOp) -> Option<JsPrim> {
    match op {
        BinaryOp::Add => Some(JsPrim::Add),
        BinaryOp::Sub => Some(JsPrim::Subtract),
        BinaryOp::Mul => Some(JsPrim::Multiply),
        BinaryOp::Div => Some(JsPrim::Divide),
        BinaryOp::Rem => Some(JsPrim::Remainder),
        BinaryOp::Exponent => Some(JsPrim::Exponent),
        BinaryOp::Less => Some(JsPrim::LessThan),
        BinaryOp::Greater => Some(JsPrim::GreaterThan),
        BinaryOp::LessEqual => Some(JsPrim::LessOrEqual),
        BinaryOp::GreaterEqual => Some(JsPrim::GreaterOrEqual),
        BinaryOp::BitAnd => Some(JsPrim::BitAnd),
        BinaryOp::BitOr => Some(JsPrim::BitOr),
        BinaryOp::BitXor => Some(JsPrim::BitXor),
        BinaryOp::Shl => Some(JsPrim::ShiftLeft),
        BinaryOp::Shr => Some(JsPrim::ShiftRight),
        BinaryOp::UShr => Some(JsPrim::ShiftRightUnsigned),
        BinaryOp::StrictEqual => Some(JsPrim::StrictEquals),
        BinaryOp::LooseEqual => Some(JsPrim::LooseEquals),
        BinaryOp::InstanceOf => Some(JsPrim::InstanceOf),
        BinaryOp::In => Some(JsPrim::HasProperty),
        // Every other operator is a row the table does not have.
        //
        // The three comparisons above are rows of their OWN, which is what changed:
        // they were refused because `Greater` is not `LessThan` with the operands
        // swapped -- `a > b` coerces `a` first and `b < a` coerces `b` first, which
        // is observable -- and that argument refuses the REWRITE, never the
        // operation. Giving each its own row costs one table entry and keeps the
        // order the program wrote.
        _ => None,
    }
}

/// The name a refusal reports for a statement.
pub(super) fn name_of(kind: &StmtKind) -> &'static str {
    match kind {
        StmtKind::Break(_) => "break",
        StmtKind::Continue(_) => "continue",
        StmtKind::Labelled { .. } => "a label",
        StmtKind::Switch { .. } => "switch",
        StmtKind::With { .. } => "with",
        StmtKind::Using { .. } => "using",
        StmtKind::Debugger => "debugger",
        _ => "a statement kind",
    }
}

/// The name a refusal reports for an expression.
///
/// # Why there is no fall-through arm any more
///
/// There was one, and it read "an expression kind" — which is what 43 refusals in
/// `bench/` and 26 in `tests/` said, the biggest single bucket in one of them. A
/// refusal that does not name itself is worth the same as no refusal at all: the
/// whole reason this enum has one variant per reason is that the survey is the work
/// queue, and a bucket cannot be queued.
///
/// So every variant is listed, and a node added to the tree tomorrow fails to
/// compile here rather than joining a bucket.
pub(super) fn expression_name(kind: &ExprKind) -> &'static str {
    match kind {
        ExprKind::Call { .. } => "a call",
        ExprKind::Member { .. } | ExprKind::Index { .. } => "a property access",
        ExprKind::Assign { .. } => "an assignment",
        ExprKind::Update { .. } => "an increment",
        ExprKind::Function(_) => "a function expression",
        ExprKind::Object { .. } => "an object literal",
        ExprKind::Array { .. } => "an array literal",
        ExprKind::This => "this",
        ExprKind::Unary { .. } => "a unary operator",
        ExprKind::Logical { .. } => "a short-circuiting operator",
        ExprKind::Conditional { .. } => "a conditional",
        ExprKind::New { .. } => "a construction",
        ExprKind::Await(_) => "await",
        ExprKind::Yield { .. } => "yield",
        ExprKind::Template { .. } => "a template literal",
        ExprKind::TaggedTemplate { .. } => "a tagged template",
        ExprKind::Chain(_) => "an optional chain",
        ExprKind::Sequence { .. } => "a comma expression",
        ExprKind::Class(_) => "a class expression",
        ExprKind::SuperMember { .. } => "a super property",
        ExprKind::SuperCall { .. } => "a super call",
        ExprKind::PrivateName(_) => "a private name",
        ExprKind::NewTarget => "new.target",
        ExprKind::ImportMeta => "import.meta",
        ExprKind::ImportCall { .. } => "a dynamic import",
        ExprKind::Asserted { .. } => "a type assertion",
        // Reached only where the arm above refused before naming: a literal and an
        // identifier both lower, so their names exist for a refusal raised about
        // something inside them.
        ExprKind::Literal(_) => "a literal of another kind",
        ExprKind::Ident(_) => "an identifier",
        ExprKind::Binary { .. } => "a binary operator",
    }
}
