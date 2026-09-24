//! A declaration statement: `let`, `const`, `var`, and what each binds.
//!
//! Apart from the lowering because `mod.rs` passed this crate's ceiling of 1000 lines,
//! and a declaration is a seam of its own: the one statement that INTRODUCES bindings
//! rather than reading or rebinding them.

use super::{Lowering, Unsupported};
use crate::domain::Type;
use crate::syntax::{Binding, BindingKind, Expr, ExprKind, Pattern, Stmt};
use crate::values::Singleton;

impl Lowering<'_> {
    /// Lowers one declaration statement.
    pub(super) fn declare(
        &mut self,
        bindings: &[Binding],
        kind: BindingKind,
        statement: &Stmt,
    ) -> Result<bool, Unsupported> {
        for Binding { target, value, .. } in bindings {
            // A PATTERN needs a value to read from, so a declaration with
            // no initialiser cannot have one -- and the language agrees: a
            // destructuring declaration must be initialised.
            if !matches!(target, Pattern::Name(_)) {
                let Some(expr) = value else {
                    return Err(Unsupported::Statement(
                        "a destructuring declaration with no initialiser",
                    ));
                };
                let held = self.expression(expr)?;
                self.destructure(target, held, expr)?;
                continue;
            }
            let Pattern::Name(name) = target else {
                unreachable!("the arm above took every other shape")
            };
            // `var x;` DOES NOTHING when it runs: the binding was hoisted to
            // the top of the function and holds whatever was assigned before
            // this line. Rebinding it to `undefined` here made
            // `x = 3; var x; return x` answer `undefined`, which a program that
            // ran through this stage showed at once.
            if value.is_none()
                && kind == BindingKind::Var
                && self
                    .resolution
                    .binding_in(self.scope, *name)
                    .is_some_and(|held| {
                        // Or it lives in an environment, which defined it as
                        // `undefined` when it was built.
                        self.values.contains_key(&held) || self.resolution.captured(held)
                    })
            {
                continue;
            }
            let (held, of) = match value {
                Some(expr) => {
                    let held = self.expression(expr)?;
                    let of = self.type_of(held);
                    (held, of)
                }
                // `let x;` is `undefined`, which is a value like any
                // other.
                None => {
                    let held = self.singleton(Singleton::Undefined, statement);
                    (held, Type::Undefined)
                }
            };
            // A DECLARATION is always this function's -- a `let` binds
            // here -- so the position is only carried for the arm that
            // cannot be reached from one.
            let at = Expr {
                kind: ExprKind::Ident(*name),
                at: statement.at,
            };
            self.bind(*name, held, of, &at)?;
        }
        Ok(false)
    }
}
