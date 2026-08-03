//! Statements, declarations, and functions.

use rts_cranelift::fault::Position;

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
    /// What it is called.
    pub name: Name,
    /// What it starts as, if anything.
    ///
    /// Absent means `undefined` for `var` and `let`, and is not legal for
    /// `const` — which is a rule about the language rather than about the tree,
    /// so the tree can express it and something else rejects it.
    pub value: Option<Expr>,
    /// What the program claimed it holds.
    pub claim: Option<Claim>,
}

/// What catches a thrown value.
#[derive(Clone, PartialEq, Debug)]
pub struct Catch {
    /// What it calls the value, if it names it.
    pub binding: Option<Name>,
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
    /// What it is called.
    pub name: Name,
    /// What it is when the caller passed nothing.
    ///
    /// A default is evaluated at the call rather than at the declaration, and
    /// only when the argument was absent — which is why it is an expression kept
    /// here rather than a value computed once.
    pub default: Option<Expr>,
    /// Whether it gathers everything the caller passed after this point.
    pub rest: bool,
    /// What the program claimed it holds.
    pub claim: Option<Claim>,
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
