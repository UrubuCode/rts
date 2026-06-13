//! A small typed IR for the P1 proofs — `Node` + `Func`.
//!
//! This is the minimal slice of the future HIR sufficient to prove the value
//! model end-to-end: enough to build `x*x + 1.0` unboxed, an identity over a
//! `Tagged` PolyValue, explicit box/unbox coercions, and `CallExtern` to the
//! runtime ops. Every node carries — or produces a value of — a [`Repr`]
//! ([`crate::repr::Repr`]): the representation is a property of the IR, not a
//! side-table.

use crate::repr::Repr;

/// One IR node. Each evaluates to a single SSA value with a known [`Repr`].
#[derive(Clone, Debug)]
pub enum Node {
    /// A literal `f64` (repr `Float64`).
    ConstF64(f64),
    /// A literal `i32` (repr `Int32`).
    ConstI32(i32),
    /// A literal `PolyValue` raw word (repr `Tagged`). Used to splice a built
    /// value (a string handle, a bool, a vec handle) into the IR as a constant.
    ConstPoly(u64),
    /// Function parameter `n`. Its repr is `Func::params[n]`.
    Param(usize),

    /// `a + b`, representation-aware:
    /// - both `Float64` → native `fadd`,
    /// - both `Int32` → native `iadd`,
    /// - otherwise → BOX both operands and `CallExtern("__rtsadp_add", ..)`
    ///   (the generic JS `+`), yielding a `Tagged` result.
    Add(Box<Node>, Box<Node>),

    /// Multiply, same repr rules as [`Node::Add`] for the native arms. (The
    /// generic fallback for `*` is out of P1 scope — multiply is only used on
    /// proven-numeric operands here.)
    Mul(Box<Node>, Box<Node>),

    /// Explicit BOX coercion: take the inner value (Int32/Float64) to a `Tagged`
    /// PolyValue word. Pure Cranelift IR (no extern call).
    Box(Box<Node>),

    /// Explicit UNBOX coercion: take a `Tagged` PolyValue word to the given
    /// unboxed `Repr` (`Int32` or `Float64`). Pure Cranelift IR.
    Unbox(Box<Node>, Repr),

    /// Call a runtime extern by symbol. All args and the return are `Tagged`
    /// (raw PolyValue `u64`); the result repr is `Tagged`.
    CallExtern(&'static str, Vec<Node>),

    /// Return the inner value from the function.
    Return(Box<Node>),
}

/// A whole function: typed params, a return repr, and a body (a sequence of
/// nodes; the last meaningful one is a `Return`).
#[derive(Clone, Debug)]
pub struct Func {
    pub params: Vec<Repr>,
    pub ret: Repr,
    pub body: Vec<Node>,
}

impl Func {
    /// Helper: a single-`Return` function over `params`/`ret`.
    pub fn single(params: Vec<Repr>, ret: Repr, expr: Node) -> Func {
        Func { params, ret, body: vec![Node::Return(Box::new(expr))] }
    }
}

// -- small Node constructors to keep test/IR construction readable --

impl Node {
    pub fn add(a: Node, b: Node) -> Node {
        Node::Add(Box::new(a), Box::new(b))
    }
    pub fn mul(a: Node, b: Node) -> Node {
        Node::Mul(Box::new(a), Box::new(b))
    }
    pub fn boxed(inner: Node) -> Node {
        Node::Box(Box::new(inner))
    }
    pub fn unbox(inner: Node, to: Repr) -> Node {
        Node::Unbox(Box::new(inner), to)
    }
}
