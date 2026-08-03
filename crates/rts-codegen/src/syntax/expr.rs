//! Expressions.

use rts_cranelift::fault::Position;

use super::Claim;
use crate::names::Name;
use crate::values::Singleton;

/// An operator with two operands.
///
/// One list, not one per category, because the interesting fact about most of
/// them is the same: what they mean depends on what they are given, and the
/// language decides that at run time unless something proved otherwise.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BinaryOp {
    /// `+`, which is addition or concatenation and does not decide until it runs.
    Add,
    /// `-`, `*`, `/`, `%`: arithmetic, whatever it is given.
    Sub,
    /// Multiplication.
    Mul,
    /// Division.
    Div,
    /// Remainder.
    Rem,
    /// `===`, which compares without converting.
    StrictEqual,
    /// `!==`.
    StrictNotEqual,
    /// `==`, which converts first, by rules with no shorter description than
    /// themselves.
    LooseEqual,
    /// `!=`.
    LooseNotEqual,
    /// `<`.
    Less,
    /// `<=`.
    LessEqual,
    /// `>`.
    Greater,
    /// `>=`.
    GreaterEqual,
}

impl BinaryOp {
    /// Whether this operator converts its operands before comparing.
    ///
    /// The distinction `===` exists for. Kept as a question about the operator
    /// rather than a branch at each use, so that a lowering asking "does this
    /// convert" gets one answer everywhere.
    pub fn converts(self) -> bool {
        matches!(self, BinaryOp::LooseEqual | BinaryOp::LooseNotEqual)
    }

    /// Whether both operands being numbers makes this arithmetic.
    ///
    /// True of everything except the equalities, which compare rather than
    /// compute. `+` is included: on two numbers it adds, and the only reason it
    /// is interesting is that it does something else otherwise.
    pub fn is_arithmetic(self) -> bool {
        matches!(
            self,
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem
        )
    }
}

/// An operator with one operand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnaryOp {
    /// `-`.
    Negate,
    /// `!`, which asks whether something is falsy and answers the opposite.
    Not,
    /// `typeof`, which answers for anything, including a name that does not
    /// exist — the one place reading an undeclared binding is not an error.
    TypeOf,
    /// `void`, which evaluates and discards.
    Void,
}

/// How the operands of a logical operator relate.
///
/// Separate from [`BinaryOp`] because they do not evaluate both sides. Modelling
/// them as ordinary binary operators would put a special case in every lowering
/// that walks one, to undo a shape that was wrong to begin with.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LogicalOp {
    /// `&&`: the left, if it is falsy; otherwise the right.
    And,
    /// `||`: the left, if it is truthy; otherwise the right.
    Or,
    /// `??`: the left, unless it is null or undefined.
    ///
    /// Not `||` with a different threshold: it distinguishes absent from falsy,
    /// which is the entire reason it was added to the language.
    Coalesce,
}

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

    /// Assignment.
    Assign {
        /// What is assigned to.
        target: Box<Expr>,
        /// What is assigned.
        value: Box<Expr>,
        /// The operator, for the compound forms.
        ///
        /// `a += b` is not `a = a + b`: the target is evaluated once. Carrying
        /// the operator here rather than rewriting keeps that true.
        op: Option<BinaryOp>,
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
