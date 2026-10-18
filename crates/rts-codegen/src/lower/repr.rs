//! `@repr(i32)`: a parameter that is a raw int32, whose arithmetic wraps.
//!
//! # What it means, and why that is the feature and not a bug
//!
//! `function mix(@repr(i32) h: number, @repr(i32) c: number)` declares `h` and `c` to be
//! 32-bit integers in the asm.js sense: every `+ - *` between them WRAPS modulo 2^32, as
//! if the author had written `|0` after each operation. The language's own `+` does not
//! -- `2147483647 + 1` is `2147483648` -- so this deliberately diverges from the f64
//! answer where a value overflows. The correctness oracle is the explicit `|0` version of
//! the same function (`(h * 31 + c) | 0`), which this agrees with bit for bit, because it
//! is lowered AS that version: a `ToInt32` after each arithmetic operation, which the
//! int32 machinery (`fold::to_int32_answer`, the `Type::Int32` compares) already turns
//! into `IntArith` and integer compares. Nothing here is a new operation.
//!
//! # Why it is std-only
//!
//! Wrapping where the program wrote `+` is a different answer. The std author opts a value
//! into it on purpose, knowing the range; user code must not get it by writing a
//! decorator, so `emit/function.rs` refuses it outside `Origin::Std`, as it does
//! `@intrinsic`.
//!
//! # What is wrapped, exactly
//!
//! - The entry: the argument is `ToInt32`d, so the binding is an `Int32` for good.
//! - Every WRITE to the binding (`=`, `+=`, `++`, a default) is `ToInt32`d, in
//!   `write_binding`, so the invariant "it holds an int32" cannot be broken by an assignment.
//! - `a + b`, `a - b`, `a * b` where BOTH operands are raw-int32 expressions: a parameter
//!   of this kind, an integer literal, a bitwise result (always an int32), or another such
//!   `+ - *`. A mixed operation (`h + someDouble`) is the language's ordinary one -- it
//!   was not declared i32, and silently wrapping it would change what the author wrote.
//!
//! Compares need nothing: `<` over two `Int32` operands already takes the integer path.
//! Negation, division and `>>>` are not wrapped (`-h` of the minimum int32 is not an
//! int32 in the language); a closure that captures the parameter reads an ordinary
//! number, since its body is lowered on its own.

use rts_mir::cfg::ValueId;

use super::{Lowering, Unsupported};
use crate::domain::JsPrim;
use crate::syntax::{BinaryOp, Expr, ExprKind, Literal};

impl Lowering<'_> {
    /// `ToInt32(value)`, spelled as the `value | 0` the oracle is.
    pub(super) fn to_int32(&mut self, value: ValueId, at: &Expr) -> Result<ValueId, Unsupported> {
        let zero = self.literal(&Literal::Number(0.0), at)?;
        Ok(self.prim(JsPrim::BitOr, vec![value, zero], at))
    }

    /// Whether `left op right` is a wrapping operation: `op` is `+ - *` and both operands
    /// are raw int32s. Cheap when no parameter is declared (`repr_i32` is empty).
    pub(super) fn wraps(&self, op: BinaryOp, left: &Expr, right: &Expr) -> bool {
        !self.repr_i32.is_empty()
            && matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul)
            && self.is_raw_int32(left)
            && self.is_raw_int32(right)
    }

    /// Whether the expression is, by its SYNTAX, a raw int32 of this function.
    fn is_raw_int32(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Ident(name) => {
                self.substituted_name(*name).is_none()
                    && self
                        .resolution
                        .binding_in(self.scope, *name)
                        .is_some_and(|binding| self.repr_i32.contains(&binding))
            }
            ExprKind::Literal(Literal::Number(held)) => {
                held.fract() == 0.0 && i32::try_from(*held as i64).is_ok()
            }
            ExprKind::Asserted { value, .. } => self.is_raw_int32(value),
            ExprKind::Binary { op, left, right } => match op {
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
                    self.is_raw_int32(left) && self.is_raw_int32(right)
                }
                // A bitwise result is an int32 whatever its operands were.
                BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::BitXor
                | BinaryOp::Shl
                | BinaryOp::Shr => true,
                _ => false,
            },
            _ => false,
        }
    }
}
