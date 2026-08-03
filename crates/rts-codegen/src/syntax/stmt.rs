//! Statements, declarations, and functions.

use rts_cranelift::fault::Position;

use super::pattern::Pattern;
use super::{Claim, Expr};
use crate::names::Name;

/// How a binding behaves.
///
/// Three, not two, because the language has three and they differ in ways a
/// lowering has to know: whether the name exists before its declaration runs,
/// whether it can be assigned again, and what a closure capturing it in a loop
/// captures.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BindingKind {
    /// `var`: reaches the enclosing function, and exists — as `undefined` —
    /// from the moment that function is entered.
    Var,
    /// `let`: reaches the enclosing block, and reading it before its declaration
    /// runs is an error rather than `undefined`.
    Let,
    /// `const`: like `let`, and assigning to it again is an error.
    ///
    /// It says the *binding* does not change, not the value. A `const` object
    /// can have its properties written all day, and a lowering that concluded
    /// otherwise would be optimizing against the language.
    Const,
}

impl BindingKind {
    /// Whether the name exists before its declaration is reached.
    pub fn exists_before_declared(self) -> bool {
        matches!(self, BindingKind::Var)
    }

    /// Whether it may be assigned again.
    pub fn is_assignable(self) -> bool {
        !matches!(self, BindingKind::Const)
    }

    /// Whether it belongs to the enclosing block rather than the function.
    pub fn is_block_scoped(self) -> bool {
        !matches!(self, BindingKind::Var)
    }
}

/// A statement, and where it came from.
#[derive(Clone, PartialEq, Debug)]
pub struct Stmt {
    /// What it is.
    pub kind: StmtKind,
    /// Where in the program it was written.
    pub at: Position,
}

impl Stmt {
    /// A statement at a position.
    pub fn new(kind: StmtKind, at: Position) -> Self {
        Self { kind, at }
    }
}

/// What a statement is.
#[derive(Clone, PartialEq, Debug)]
pub enum StmtKind {
    /// An expression evaluated for what it does.
    Expr(Expr),

    /// One or more bindings introduced together.
    Declare {
        /// How they behave.
        kind: BindingKind,
        /// The bindings.
        bindings: Vec<Binding>,
    },

    /// A sequence, and a scope if what it holds is block-scoped.
    Block(Vec<Stmt>),

    /// A condition and one or two branches.
    If {
        /// The condition, read for truthiness rather than compared to anything.
        condition: Expr,
        /// Run when it is truthy.
        then_branch: Box<Stmt>,
        /// Run otherwise, if there is one.
        else_branch: Option<Box<Stmt>>,
    },

    /// A condition checked before each pass.
    While {
        /// The condition.
        condition: Expr,
        /// The body.
        body: Box<Stmt>,
    },

    /// Leaving a function, with a value or without.
    ///
    /// Without is not the same as returning nothing: it returns `undefined`,
    /// which is a value. Modelled as an absent expression rather than an implicit
    /// one so that the place that decides what it means is the lowering.
    Return(Option<Expr>),

    /// Leaving a loop.
    Break,

    /// Skipping to the next pass of a loop.
    Continue,

    /// Raising a value.
    Throw(Expr),

    /// A protected region, with a handler, a cleanup, or both.
    Try {
        /// What is protected.
        body: Vec<Stmt>,
        /// What catches, and what it calls the value it caught.
        ///
        /// The binding is optional: `catch {}` without one is legal, and a tree
        /// that required one would have to invent a name nothing can refer to.
        catch: Option<Catch>,
        /// What runs on the way out, however that happens.
        finally: Option<Vec<Stmt>>,
    },

    /// A function declared by name.
    Function(Box<Function>),

    /// Nothing.
    Empty,
}

/// One binding introduced by a declaration.
#[derive(Clone, PartialEq, Debug)]
pub struct Binding {
    /// What it introduces — a name, or a pattern that introduces several.
    ///
    /// Must satisfy [`Pattern::is_valid_binding`]: a declaration cannot write
    /// into `obj.x`, though a destructuring assignment can.
    pub target: Pattern,
    /// What it starts as, if anything.
    ///
    /// Absent means `undefined` for `var` and `let`, and is not legal for
    /// `const` — nor for any pattern, which has nothing to destructure. Both are
    /// rules about the language rather than about the tree, so the tree can
    /// express them and something else rejects them.
    pub value: Option<Expr>,
    /// What the program claimed it holds.
    pub claim: Option<Claim>,
}

/// What catches a thrown value.
#[derive(Clone, PartialEq, Debug)]
pub struct Catch {
    /// What it calls the value, if it names it.
    ///
    /// A pattern, because `catch ({ message })` is legal. Absent because
    /// `catch {}` is too.
    pub binding: Option<Pattern>,
    /// What it does.
    pub body: Vec<Stmt>,
}

/// A function, however it was written.
///
/// One shape for declarations, expressions and arrows, because they differ in
/// two facts rather than in structure — what `this` means inside them, and
/// whether the name exists before the declaration runs. Two shapes would mean
/// two of everything that walks one.
#[derive(Clone, PartialEq, Debug)]
pub struct Function {
    /// What it is called, if it has a name.
    pub name: Option<Name>,
    /// Its parameters, in order.
    pub parameters: Vec<Parameter>,
    /// `...rest`, which gathers every argument past the declared ones.
    ///
    /// Its own field rather than a flag on the last parameter, so that "rest is
    /// last" and "rest has no default" are facts about the type instead of rules
    /// somebody has to enforce.
    pub rest_parameter: Option<Pattern>,
    /// Its body.
    pub body: Vec<Stmt>,
    /// What the program claimed it returns.
    pub returns: Option<Claim>,
    /// Whether it takes `this` from where it was written rather than from how it
    /// is called.
    ///
    /// The one thing arrows actually change. Everything else about them is
    /// spelling.
    pub captures_this: bool,
    /// Whether it may park its frame.
    pub is_async: bool,
    /// Whether it may yield.
    pub is_generator: bool,
    /// Where it was written.
    pub at: Position,
}

/// One parameter.
#[derive(Clone, PartialEq, Debug)]
pub struct Parameter {
    /// What it introduces.
    pub target: Pattern,
    /// What it is when the caller passed nothing.
    ///
    /// A default is evaluated at the call rather than at the declaration, and
    /// only when the argument was `undefined` — which is why it is an expression
    /// kept here rather than a value computed once. Passing `undefined`
    /// explicitly triggers it; passing `null` does not.
    pub default: Option<Expr>,
    /// What the program claimed it holds.
    pub claim: Option<Claim>,
}

impl Function {
    /// Whether every parameter is a plain name with no default and there is no
    /// rest parameter.
    ///
    /// The spec calls this a "simple parameter list", and it is not a style
    /// question. A non-simple list forbids a `"use strict"` directive in the
    /// body — because the directive would change how the parameters themselves
    /// are parsed, after they have already been parsed — and it decouples the
    /// `arguments` object from the parameters, so writing `arguments[0]` stops
    /// being visible through the parameter name.
    pub fn has_simple_parameter_list(&self) -> bool {
        self.rest_parameter.is_none()
            && self
                .parameters
                .iter()
                .all(|p| p.default.is_none() && matches!(p.target, Pattern::Name(_)))
    }

    /// Every name the parameter list introduces, in order.
    pub fn parameter_names(&self) -> Vec<Name> {
        let mut names = Vec::new();
        for parameter in &self.parameters {
            parameter.target.bound_names(&mut names);
        }
        if let Some(rest) = &self.rest_parameter {
            rest.bound_names(&mut names);
        }
        names
    }
}

/// A whole program.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Program {
    /// What it does, in order.
    pub body: Vec<Stmt>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_var_exists_before_its_declaration_runs() {
        assert!(BindingKind::Var.exists_before_declared());
        assert!(!BindingKind::Let.exists_before_declared());
        assert!(!BindingKind::Const.exists_before_declared());
    }

    #[test]
    fn const_binds_the_name_and_says_nothing_about_the_value() {
        assert!(!BindingKind::Const.is_assignable());
        assert!(
            BindingKind::Const.is_block_scoped(),
            "it is let with one more rule, not a third kind of scope"
        );
    }
}
