//! Expressions.

use rts_cranelift::fault::Position;

use super::Claim;
use super::ops::{AssignOp, BinaryOp, LogicalOp, UnaryOp, UpdateOp, UpdatePosition};
use crate::names::Name;
use crate::values::Singleton;

/// A value written in the program.
#[derive(Clone, PartialEq, Debug)]
pub enum Literal {
    /// A number. Always a double, because JavaScript has one numeric type.
    Number(f64),
    /// Text.
    String(String),
    /// `true` or `false`.
    Boolean(bool),
    /// `undefined` or `null`.
    Singleton(Singleton),
}

/// An expression, and where it came from.
#[derive(Clone, PartialEq, Debug)]
pub struct Expr {
    /// What it is.
    pub kind: ExprKind,
    /// Where in the program it was written.
    ///
    /// The machine layer's opaque position, carried rather than translated: a
    /// fault that happened here has to name this, and translating it twice is
    /// two chances to name something else.
    pub at: Position,
}

impl Expr {
    /// An expression at a position.
    pub fn new(kind: ExprKind, at: Position) -> Self {
        Self { kind, at }
    }
}

/// What an expression is.
#[derive(Clone, PartialEq, Debug)]
pub enum ExprKind {
    /// A value written directly.
    Literal(Literal),

    /// A name.
    Ident(Name),

    /// Two operands and an operator.
    Binary {
        /// Which operator.
        op: BinaryOp,
        /// The left operand.
        left: Box<Expr>,
        /// The right operand.
        right: Box<Expr>,
    },

    /// One operand and an operator.
    Unary {
        /// Which operator.
        op: UnaryOp,
        /// The operand.
        operand: Box<Expr>,
    },

    /// An operator that may not evaluate its right side.
    Logical {
        /// Which operator.
        op: LogicalOp,
        /// Always evaluated.
        left: Box<Expr>,
        /// Evaluated only if the left did not decide the answer.
        right: Box<Expr>,
    },

    /// A property named in the program: `a.b`.
    ///
    /// Separate from the computed form because the key is known here and is not
    /// there, and everything that follows differs on exactly that. A tree that
    /// made them one node would push the difference into every reader.
    Member {
        /// What is read.
        object: Box<Expr>,
        /// Which property.
        property: Name,
        /// Whether `?.` — the object being absent yields absent instead of
        /// failing.
        optional: bool,
    },

    /// A property computed at run time: `a[e]`.
    Index {
        /// What is read.
        object: Box<Expr>,
        /// The expression naming the property, evaluated first.
        index: Box<Expr>,
        /// Whether `?.[`.
        optional: bool,
    },

    /// A call.
    Call {
        /// What is called.
        callee: Box<Expr>,
        /// The arguments, in order.
        arguments: Vec<Expr>,
        /// Whether `?.(` — an absent callee yields absent instead of failing.
        optional: bool,
    },

    /// Construction: `new f(...)`.
    ///
    /// Not a call with a flag. It builds an object, decides what that object's
    /// prototype is, and treats the result differently depending on what the
    /// callee returned — none of which a call does.
    New {
        /// What is constructed.
        callee: Box<Expr>,
        /// The arguments, in order.
        arguments: Vec<Expr>,
    },

    /// An object written out: `{ a: 1 }`.
    Object {
        /// Its properties, in the order written — which is the order they are
        /// added, which is what decides the layout.
        properties: Vec<Property>,
    },

    /// An array written out.
    Array {
        /// Its elements. An absent one is a hole, which is not `undefined`:
        /// a hole is skipped by some operations and read as `undefined` by
        /// others, and collapsing the two loses that.
        elements: Vec<Option<Expr>>,
    },

    /// `++x` or `x--`.
    ///
    /// Its own node rather than an assignment with a constant, because it is
    /// not one: it reads the target, coerces through ToNumeric, adds, and
    /// stores — and what the *expression* yields depends on which side the
    /// operator was written. Rewriting `x++` to `x = x + 1` gets the value
    /// wrong; rewriting it to `x += 1` gets it wrong in a subtler way.
    Update {
        /// Increment or decrement.
        op: UpdateOp,
        /// Prefix yields the new value, postfix the old one.
        position: UpdatePosition,
        /// What is read and written. Must be a simple target.
        target: Box<Expr>,
    },

    /// Assignment, in all three of its forms.
    Assign {
        /// What is assigned to. A pattern is only legal under [`AssignOp::Plain`].
        target: Box<Expr>,
        /// What is assigned. Not evaluated at all by the logical forms when the
        /// target already decided.
        value: Box<Expr>,
        /// Plain, compound, or logical.
        ///
        /// `a += b` is not `a = a + b`: the target is evaluated once. Carrying
        /// the operator here rather than rewriting keeps that true.
        op: AssignOp,
    },

    /// `a, b` — evaluates each, yields the last.
    ///
    /// A list rather than nested pairs. The operator is left-associative and
    /// wholly uninteresting apart from its order, so nesting would record a
    /// shape nobody reads and force every walker to flatten it back.
    Sequence {
        /// Every operand, in order. Two or more.
        operands: Vec<Expr>,
    },

    /// A condition and two answers.
    Conditional {
        /// The condition.
        condition: Box<Expr>,
        /// Evaluated when it is truthy.
        then_branch: Box<Expr>,
        /// Evaluated otherwise.
        else_branch: Box<Expr>,
    },

    /// A function written as an expression.
    Function(Box<super::Function>),

    /// An expression whose type was asserted: `e as T`.
    ///
    /// The assertion is kept rather than applied, because it is a claim and not a
    /// proof — the program said so, and nothing checked. What it is worth is
    /// decided where claims are weighed, not here.
    Asserted {
        /// The expression.
        value: Box<Expr>,
        /// What was claimed about it.
        claim: Claim,
    },
}

/// One property of an object written out.
#[derive(Clone, PartialEq, Debug)]
pub struct Property {
    /// Which property.
    pub key: PropertyKey,
    /// What it is set to.
    pub value: Expr,
}

/// How a property of an object literal is named.
#[derive(Clone, PartialEq, Debug)]
pub enum PropertyKey {
    /// Written in the program.
    Named(Name),
    /// Computed: `{ [e]: v }`.
    Computed(Expr),
}
