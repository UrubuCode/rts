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
            if kind == BindingKind::Const
                && let Some(expr) = value
            {
                self.remember_arrow(*name, expr);
            }
        }
        Ok(false)
    }

    /// Every `var` of this function, `undefined` from its first statement -- which is
    /// the language's hoisting, and what `emit/function.rs::hoist_vars` does for the
    /// running engine. Without it a `var` first assigned inside a loop, a `switch` or a
    /// branch had no value before the construct, so nothing could carry it past, and
    /// one read before its line was refused as a dead zone it does not have.
    ///
    /// A captured one lives in the environment, which defined it as `undefined` when
    /// it was built; a parameter of the same name IS the parameter, and holds its
    /// argument.
    pub(super) fn hoist_vars(&mut self, at: &Expr) {
        let scope = self.resolution.scope(self.function);
        let hoisted: Vec<_> = scope
            .bindings
            .iter()
            .copied()
            .filter(|held| {
                self.resolution.binding(*held).origin == crate::names::resolve::Origin::Var
                    && !self.resolution.captured(*held)
                    && !self.values.contains_key(held)
            })
            .collect();
        if hoisted.is_empty() {
            return;
        }
        let undefined = self.singleton_at(Singleton::Undefined, at);
        for held in hoisted {
            self.values.insert(held, undefined);
        }
    }

    /// A nested definition: a closure bound to its name, and the name is this
    /// function's -- so it is an ordinary rebind, or an environment write where a
    /// closure captures it.
    pub(super) fn declare_function(&mut self, function: &crate::syntax::Function) -> Result<(), Unsupported> {
        let Some(name) = function.name else {
            return Err(Unsupported::Statement("a function declaration with no name"));
        };
        let Some(id) = self.callees.of_position(function.at) else {
            return Err(Unsupported::Statement(
                "a nested definition needs the module's numbering",
            ));
        };
        let at = Expr {
            kind: ExprKind::Ident(name),
            at: function.at,
        };
        let held = self.closure(id, &at);
        let of = self.type_of(held);
        self.bind(name, held, of, &at)
    }

    /// Annex B.3.3: where a function declared in a block also has a `var` of its
    /// function (`names::resolve::annex`), that `var` takes the block's binding when
    /// the declaration is evaluated.
    pub(super) fn annex_write(&mut self, function: &crate::syntax::Function) -> Result<(), Unsupported> {
        let (Some(name), Some(var)) = (function.name, self.resolution.annex_b(function.at)) else {
            return Ok(());
        };
        let Some(block) = self.resolution.binding_in(self.scope, name) else {
            return Ok(());
        };
        let at = Expr {
            kind: ExprKind::Ident(name),
            at: function.at,
        };
        let held = self.read_binding(block, name, &at)?;
        let of = self.type_of(held);
        self.write_binding(var, held, of, &at)
    }
}
