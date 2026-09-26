//! The graph: blocks, values, and the four things an instruction can be.
//!
//! SSA with block parameters rather than phi nodes, which is what the machine
//! layer underneath already does — two representations of the same idea, one of
//! them translated at every boundary, is the cost this avoids.
//!
//! # Why the operation set is this small
//!
//! Five operations, and none of them is arithmetic. Arithmetic is not neutral:
//! one language's `+` adds numbers or concatenates depending on its own coercion
//! rules, another's `..` concatenates and its `+` never does, and a third
//! distinguishes integer addition from float addition as separate operations over
//! separate types. An IR shared by two languages that named `Add` would be
//! naming one of them.
//!
//! So every operation a language performs is a [`Prim`] — an opaque index into a
//! table the language registered, carrying an [`Effect`] and a transfer function.
//! This crate composes primitives; it does not know what any of them compute.
//! README rule 4.

use rts_cranelift::fault::Position;

use crate::effect::Effect;
use crate::guard::{Assertion, PointId, Tier};

/// A block of the graph.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub struct BlockId(pub u32);

/// A value, defined exactly once.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub struct ValueId(pub u32);

/// An instruction, in the function's flat list.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub struct InstId(pub u32);

/// An operation the language declared: opaque here, by rule 4.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub struct Prim(pub u32);

/// An entry point the language names, called rather than emitted.
///
/// The counterpart of `rts-core`'s entry points, and the reason a `ToBoolean` is
/// not an operation here: two languages have the same ABI shape for it and
/// different answers, so the name is the interface and the language owns it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub struct EntryId(pub u32);

/// A function of the program, by index.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub struct FuncId(pub u32);

/// A constant, in the few shapes every language has.
///
/// Deliberately not "every constant a language has": a language's own singletons,
/// symbols and interned strings are constants of ITS domain, reached through a
/// [`Prim`] with no arguments or through a declared constant index. What is here
/// is what the *structure* needs — a number to index with, a truth value to
/// branch on.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Const {
    /// A signed integer.
    Int(i64),
    /// A double.
    Float(f64),
    /// A truth value, for the condition of a branch.
    Bool(bool),
    /// A constant the language declared, by index into its own table.
    Declared(u32),
}

/// What an instruction is.
#[derive(Clone, PartialEq, Debug)]
pub enum Op {
    /// A constant.
    Const(Const),
    /// A language operation, with its arguments.
    Prim { prim: Prim, args: Vec<ValueId> },
    /// A call to a named entry point, or to a function of this program, or to
    /// whatever a value holds.
    Call {
        /// What is reached.
        callee: Callee,
        /// The receiver, where the language passes one.
        ///
        /// # Why a field and not argument zero
        ///
        /// Passing it as the first argument is the convention every real calling
        /// convention uses, and it is the wrong shape HERE. A convention is an
        /// agreement held in two places — the front end that packs it and the
        /// lowering that unpacks it — and `docs/engine/deopt-lateral.md` records
        /// what those do: they drift, and the drift compiles.
        ///
        /// A field cannot drift. A lowering that forgets the receiver fails to
        /// compile instead of passing a receiver as the first argument to a
        /// function that declares none, and a pass that counts arguments counts
        /// what the program wrote.
        ///
        /// It is also the honest place for the machine question to be ASKED rather
        /// than answered: how a receiver reaches a callee is the machine's
        /// convention, and `lower/` is where that is decided — with the receiver
        /// still visible as itself when it gets there.
        receiver: Option<ValueId>,
        /// The arguments the program wrote, in order.
        args: Vec<ValueId>,
    },

    /// Hands a value out and answers what comes back.
    ///
    /// The neutral form of `yield` and of `await`: the frame parks, something outside
    /// decides what to send in, and the instruction's result is that. What the two
    /// differ about — a value versus a promise, who resumes and when — is the
    /// language's, and it declares that by which entry point it calls around this.
    ///
    /// # Why it is an instruction and not a terminator
    ///
    /// Because the CFG does not have to split here. `rts_cranelift::frame` transforms
    /// the whole function into a resumable form from LIVENESS — it spills what is live
    /// across each suspension into a record with a resume position — so a graph that
    /// split its blocks at every suspension would be doing that transform's work
    /// badly, and twice.
    ///
    /// What the graph must carry is that control leaves, and it carries it in the
    /// [`Effect::SUSPENDS`] flag where every pass already looks.
    Suspend {
        /// What is handed out, if anything. `yield` with no value hands out nothing.
        value: Option<ValueId>,
    },
    /// An assertion about a value, checked, with somewhere to fall when it fails.
    ///
    /// Its result is the same value with a narrowed type — which is what makes a
    /// guard visible to CSE, hoisting and LICM instead of being emission. README
    /// rule 6.
    Guard {
        /// What is asserted.
        assertion: Assertion,
        /// About which value.
        on: ValueId,
        /// Where the fall lands, in the other tier.
        point: PointId,
    },
}

/// What a call reaches.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Callee {
    /// A named entry point of the runtime.
    Entry(EntryId),
    /// Another function of this program.
    Func(FuncId),
    /// Whatever the value holds, which is everything a program can do.
    Dynamic(ValueId),
}

/// An instruction: what it does, where it came from, and what it does besides
/// answer.
#[derive(Clone, PartialEq, Debug)]
pub struct Inst {
    /// The operation.
    pub op: Op,
    /// The value it defines.
    pub result: ValueId,
    /// Where it was written.
    pub at: Position,
    /// What it does besides answer.
    ///
    /// Carried on the instruction rather than looked up from the primitive table
    /// at every use, because a pass asks this of every instruction it considers
    /// moving and the table is the language's, not this crate's.
    pub effect: Effect,
}

mod flow;
mod func;

pub use flow::Terminator;
pub use func::{Block, Func, FuncBuilder};
