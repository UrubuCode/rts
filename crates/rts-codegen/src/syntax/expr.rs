//! Expressions.

use rts_cranelift::fault::Position;

use super::Claim;
use super::ops::{AssignOp, BinaryOp, LogicalOp, UnaryOp, UpdateOp, UpdatePosition};
use super::pattern::Pattern;
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

    /// `/ab+c/gi` — the pattern and the flags, exactly as written.
    ///
    /// # Why the pattern is text and not a parsed form
    ///
    /// Because the grammar of a regular expression is its own, and the point at
    /// which it is read is a decision about **when** rather than about how: a
    /// literal is compiled once, and a `new RegExp(s)` cannot be compiled until
    /// the string exists. Holding the source keeps both on one path.
    ///
    /// It is also what the specification does — a literal carries its pattern
    /// and flags as text, and an early error checks them without the value
    /// existing.
    Regex {
        /// Between the slashes, unescaped.
        pattern: String,
        /// The letters after the closing slash.
        flags: String,
    },

    /// `123n` — an integer of arbitrary size.
    ///
    /// # Why the digits and not a number
    ///
    /// A `BigInt` is exactly the thing a `f64` cannot hold, so parsing it into
    /// one here would lose the literal in the act of recording it. The text is
    /// kept and whatever implements the arithmetic decides the representation —
    /// which is the same reasoning `Regex` above uses, and for a sharper reason:
    /// there is no lossless machine type to choose yet.
    BigInt(String),
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
        arguments: Vec<Spreadable>,
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
        arguments: Vec<Spreadable>,
    },

    /// `await e` — parks the frame until a promise settles.
    ///
    /// Its own node, and not a call, because it is the only expression that can
    /// end a frame's residence on the stack: everything live across it has to be
    /// somewhere that survives, which is the machine layer's frame
    /// transformation. Note it awaits *any* value — a non-promise resolves
    /// immediately, but still after a turn of the loop, so it is never free.
    Await(Box<Expr>),

    /// `yield e` or `yield* e`.
    ///
    /// Suspends and produces. Delegation is a flag rather than a variant because
    /// the operand plays the same role in both — but the flag is load-bearing:
    /// `yield*` forwards `next`, `throw` and `return` to the inner iterator and
    /// yields whatever it yields, so it is a loop, not a single suspension.
    Yield {
        /// What is produced. Absent for a bare `yield`, which produces
        /// `undefined`.
        value: Option<Box<Expr>>,
        /// Whether `yield*`.
        delegate: bool,
    },

    /// `this`.
    ///
    /// A node rather than a name, because it does not resolve like one: it comes
    /// from the nearest enclosing function that binds it, arrows do not bind it,
    /// and what it holds is decided by how the function was *called*.
    This,

    /// `` `a${b}c` `` — the parts and the expressions between them.
    Template {
        /// The literal pieces. Always one more than `expressions`.
        parts: Vec<TemplatePart>,
        /// What is substituted between them, in order.
        expressions: Vec<Expr>,
    },

    /// `` tag`a${b}` `` — a call whose first argument is the template itself.
    ///
    /// Its own node because it is not a call of the template's result: the tag
    /// receives the raw and cooked strings as an array, and that array is
    /// **cached per call site**, so the same site hands the tag the identical
    /// object every time it runs.
    TaggedTemplate {
        /// What is called.
        tag: Box<Expr>,
        /// The literal pieces handed to it.
        parts: Vec<TemplatePart>,
        /// What is substituted, in order.
        expressions: Vec<Expr>,
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
        ///
        /// A spread here contributes however many the iterator yields, so an
        /// array literal's length is not its element count — in either
        /// direction, since a trailing comma adds none and a trailing hole
        /// adds one.
        elements: Vec<Option<Spreadable>>,
    },

    /// The boundary of an optional chain.
    ///
    /// Wraps the whole of `a?.b.c`, and is what makes the short circuit reach
    /// `.c` when `a` is absent. Without it there is nowhere to say how far the
    /// skipping goes: the `optional` flag on a link says only that *that* link
    /// may be skipped, and `a?.b.c` skips a link that carries no flag at all.
    ///
    /// It is also where two early errors attach — a tagged template and a `new`
    /// cannot appear inside a chain.
    Chain(Box<Expr>),

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
        /// What is assigned to.
        target: AssignTarget,
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

    /// A class written as an expression.
    Class(Box<super::Class>),

    /// `super.x` or `super[e]`.
    ///
    /// Not a member access on a value called `super`: it reads from the *home
    /// object's* prototype while keeping `this` as the receiver, so an inherited
    /// getter reached this way still sees the instance. Only legal inside a
    /// method — which is what having a home object means.
    SuperMember {
        /// The property, named or computed.
        ///
        /// Boxed: a computed key holds an expression, and this variant sits
        /// directly inside `ExprKind` with no collection between them.
        property: Box<PropertyKey>,
    },

    /// `super(...)`.
    ///
    /// Not a call. It runs the parent constructor and **binds `this`** in the
    /// derived constructor, which is why `this` is a ReferenceError before it
    /// and why calling it twice is an error rather than a second call.
    SuperCall {
        /// The arguments, in order.
        arguments: Vec<Spreadable>,
    },

    /// `#x` where a name is expected — only as the left operand of `in`.
    ///
    /// `#x in obj` asks whether `obj` was constructed by this class, and it is
    /// the one place a private name appears without a receiver in front of it.
    PrivateName(Name),

    /// `new.target` — the constructor `new` was invoked with, or `undefined`.
    NewTarget,

    /// `import.meta`.
    ImportMeta,

    /// `import(specifier, options)` — a dynamic import, which is a call in the
    /// grammar and produces a promise.
    ImportCall {
        /// What to load.
        specifier: Box<Expr>,
        /// The options bag, if one was written.
        options: Option<Box<Expr>>,
    },

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

/// What an assignment writes to.
///
/// Two cases rather than one expression, because a destructuring target is not
/// an expression that happens to be assignable — `[a, b]` on the left of `=` is
/// reinterpreted from an array literal into a pattern, and the two mean
/// unrelated things. Keeping them apart also makes the language's own rule
/// checkable in one place: only [`AssignOp::Plain`] accepts the second.
#[derive(Clone, PartialEq, Debug)]
pub enum AssignTarget {
    /// A single place: `x`, `obj.k`, `arr[i]`.
    Place(Box<Expr>),
    /// A destructuring pattern: `[a, b]`, `{ x, ...rest }`.
    Pattern(Pattern),
}

impl AssignTarget {
    /// Whether this target is legal under `op`.
    ///
    /// The single place the rule lives. `[a, b] += c` and `[a] ||= b` are both
    /// syntax errors, arriving there for different reasons.
    pub fn is_legal_under(&self, op: AssignOp) -> bool {
        match self {
            AssignTarget::Place(_) => true,
            AssignTarget::Pattern(_) => op.allows_pattern_target(),
        }
    }
}

/// One entry of an object literal.
///
/// Five spellings, not one with flags, because they do different things at
/// construction. A method and a `key: function(){}` differ in whether the
/// function gets a home object — which decides whether `super` works inside it.
/// A spread does not add one property but copies many. And `__proto__` in one
/// of its four spellings does not add a property at all.
#[derive(Clone, PartialEq, Debug)]
pub enum Property {
    /// `key: value`, and `{ a }`, which is the same thing written shorter.
    ///
    /// Shorthand is recorded because it is the only form that can be
    /// reinterpreted into a pattern, and because `{ a }` requires `a` to be a
    /// readable binding where `{ a: 1 }` requires nothing.
    Value {
        /// Which property.
        key: PropertyKey,
        /// What it is set to.
        value: Expr,
        /// Whether it was written as `{ a }`.
        shorthand: bool,
    },

    /// `m() {}`, `*m() {}`, `async m() {}`.
    ///
    /// Not a `Value` holding a function. A method is installed with a home
    /// object, which is what `super.x` inside it reads from; a function stored
    /// under a key has none, and `super` there is a syntax error.
    Method {
        /// Which property.
        key: PropertyKey,
        /// The function.
        function: Box<super::Function>,
    },

    /// `get k() {}`.
    Getter {
        /// Which property.
        key: PropertyKey,
        /// The function. Takes no parameters.
        function: Box<super::Function>,
    },

    /// `set k(v) {}`.
    Setter {
        /// Which property.
        key: PropertyKey,
        /// The function. Takes exactly one parameter, and no rest parameter.
        function: Box<super::Function>,
    },

    /// `...expr` — copies the source's own enumerable properties.
    ///
    /// Copies *values*, so a getter on the source runs once here and what lands
    /// is a plain data property. Not the same as inheriting.
    Spread(Expr),

    /// `__proto__: value` — sets the prototype instead of adding a property.
    ///
    /// Only this spelling. `{ __proto__ }`, `{ ["__proto__"]: v }` and
    /// `{ __proto__() {} }` are ordinary properties named `__proto__`, which is
    /// why this is a variant rather than a check on the key: four syntaxes, two
    /// meanings, and the difference is not visible in the key alone.
    Prototype(Expr),
}

/// One item of a list that may spread.
///
/// Shared by call arguments, `new` arguments and array elements, because in all
/// three the question a lowering asks is the same one: is the length of this
/// list known before the program runs.
#[derive(Clone, PartialEq, Debug)]
pub enum Spreadable {
    /// An ordinary item.
    Single(Expr),
    /// `...xs` — spread, which steps an iterator and so can contribute any
    /// number of arguments, decided at run time.
    Spread(Expr),
}

impl Spreadable {
    /// Whether the number of arguments this contributes is known before running.
    ///
    /// False for a spread. Once one appears, the call's arity is a runtime
    /// value, so parameter positions after it cannot be resolved statically —
    /// which is what a lowering needs to know before it plans the call.
    pub fn count_is_static(&self) -> bool {
        matches!(self, Spreadable::Single(_))
    }
}

/// One piece of a template literal.
///
/// Both texts are kept. Cooked is what the escapes mean; raw is what was
/// written. An ordinary template uses cooked. A *tagged* template receives
/// both, and receives cooked as `undefined` when an escape is invalid — which
/// is why cooked is optional and raw is not.
#[derive(Clone, PartialEq, Debug)]
pub struct TemplatePart {
    /// The text with escapes resolved, absent if any of them was invalid.
    pub cooked: Option<String>,
    /// The text exactly as it was written.
    pub raw: String,
}

/// How a property of an object literal is named.
#[derive(Clone, PartialEq, Debug)]
pub enum PropertyKey {
    /// Written in the program.
    Named(Name),
    /// Computed: `{ [e]: v }`.
    Computed(Expr),
}
