//! Instructions and terminators.
//!
//! Two rules shape this vocabulary, and both are visible in the shape of the
//! enum rather than enforced by convention:
//!
//! **No operation accepts both a proven and a generic operand.** Integer
//! addition and floating-point addition reject a generic operand; generic
//! arithmetic is a separate variant with a different name and a different cost.
//! There is deliberately no single "add" that inspects its operands and
//! branches — that branch, repeated at every site, is what this layer exists to
//! remove.
//!
//! **Narrowing is never implicit.** Widening to the generic form is inserted
//! automatically at merges, because it cannot fail. Narrowing out of it can fail
//! at run time, so it is only reachable through [`Terminator::Guard`], which
//! makes the failure path part of the program's structure.

use super::entity::{BlockId, ConstId, InstId, ValueId};
use super::funcs::{FuncId, SigId};
use crate::repr::Repr;
use crate::types::TypeId;
use crate::unwind::Tag;

/// Integer and floating-point comparison predicates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CmpOp {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Ge,
}

/// Arithmetic over operands whose representation is proven.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NumOp {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division.
    Div,
}

/// One-operand floating-point operations.
///
/// Only the ones the hardware does in a single instruction. A logarithm and a
/// sine are library calls on every target this crate targets, so they are not
/// here: an instruction that expands to a call would make the cost of emitting
/// one unreadable, which is the property this whole vocabulary exists for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FloatOp {
    /// Square root.
    Sqrt,
    /// Toward negative infinity.
    Floor,
    /// Toward positive infinity.
    Ceil,
    /// Toward zero.
    Trunc,
    /// Magnitude, sign cleared.
    Abs,
}

/// Bitwise operations over proven integers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BitOp {
    /// Conjunction.
    And,
    /// Disjunction.
    Or,
    /// Exclusive disjunction.
    Xor,
    /// Left shift.
    Shl,
    /// Arithmetic right shift.
    Shr,
}

/// Operations over operands whose representation is not proven.
///
/// One variant per operation, resolved at run time by the machine layer's own
/// generic implementation. A client never emits a chain of representation tests
/// itself: it either proves the representation and uses the proven operation, or
/// it does not and uses this one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GenericOp {
    /// Addition in the generic domain.
    Add,
    /// Subtraction in the generic domain.
    Sub,
    /// Multiplication in the generic domain.
    Mul,
    /// Division in the generic domain.
    Div,
    /// Comparison in the generic domain.
    Compare(CmpOp),
}

/// Where an allocation is placed.
///
/// Placement is carried by the allocation itself rather than inferred later,
/// because the collector's cost model depends on it and because a value that
/// does not leave its thread can be reclaimed without synchronizing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Region {
    /// Reachable only from the allocating thread.
    Local,
    /// Published: reachable from more than one thread.
    Shared,
}

/// An instruction.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Inst {
    /// Materializes a declared constant.
    Const(ConstId),

    /// Arithmetic over proven integers of identical representation.
    IntArith(NumOp, ValueId, ValueId),
    /// Arithmetic over proven floats of identical representation.
    FloatArith(NumOp, ValueId, ValueId),
    /// Bitwise operation over proven integers of identical representation.
    Bitwise(BitOp, ValueId, ValueId),
    /// Comparison of two proven operands of identical representation.
    Compare(CmpOp, ValueId, ValueId),

    /// A double as the 32-bit integer JavaScript's bitwise operators read it as.
    ///
    /// Takes a proven `F64` and answers a proven `I32`. It is one instruction here
    /// and a SEQUENCE in the lowering, which is the whole reason it exists as an
    /// instruction rather than as a call: the conversion is pure computation —
    /// no heap, no allocation, nothing global — and this crate's own membership
    /// rule puts pure computation in what is emitted.
    ///
    /// # Why not the code generator's own conversion
    ///
    /// Because neither of its two answers is this one. `fcvt_to_sint_sat`
    /// SATURATES, so it answers 2147483647 for 2^32 where the language says 0.
    /// `fcvt_to_sint` traps on anything out of range, and a trap in the middle of
    /// `x | 0` is the SIGILL this repository has already shipped once.
    ///
    /// The language's answer is truncation toward zero and then modulo 2^32,
    /// which the lowering builds out of a truncate, a divide, a multiply, a
    /// subtract and one saturating conversion that can no longer saturate
    /// because its operand has already been brought inside the range. NaN and
    /// both infinities fall out as zero without a branch, which is what the
    /// language asks for and what a range test would have had to special-case.
    ToInt32(ValueId),

    /// A proven 32-bit integer back as a double.
    ///
    /// The other half of [`Inst::ToInt32`], and it is not optional: a bitwise
    /// result left in the integer domain widens into a tagged INTEGER, and every
    /// numeric guard downstream tests for a double. Measured that way round —
    /// `(a * 3) | 0` in a loop kept the `|` as three instructions and turned the
    /// `*` back into `Call __rts_multiply`, because the accumulator now arrived
    /// tagged as an integer and failed the guard it used to pass.
    ///
    /// One instruction in both directions, so a language emitting bitwise work
    /// stays inside the one representation everything else here proves about.
    ToF64(ValueId),

    /// A one-operand floating-point operation the hardware performs directly.
    ///
    /// Each of these is ONE machine instruction — sqrtsd, roundsd, andpd — and
    /// each was a call into the runtime through a property read and a dispatch.
    /// Measured before this existed: 48 ns for a square root against 24 for an
    /// ordinary method call and ~0 for a multiply.
    ///
    /// The instruction says nothing about WHERE a client found the operation.
    /// Whether some language spells it `Math.sqrt` and whether that name still
    /// means this is the client's question, answered before it emits one —
    /// rule 2, no source-language knowledge here.
    FloatUnary(FloatOp, ValueId),

    /// Widens a proven value into the generic form.
    ///
    /// Emitted as pure instructions, never a call, so that a redundant pair
    /// around an already-uniform value folds away entirely.
    Widen(ValueId),
    /// Narrows a generic value whose representation a guard has established.
    Narrow(ValueId, Repr),

    /// An operation on operands whose representation is not proven.
    Generic(GenericOp, ValueId, ValueId),

    /// Reads one machine word from an address a value holds.
    ///
    /// # What it guarantees, and what it deliberately does not
    ///
    /// It guarantees exactly one load of `Repr::I64` from the address, with no
    /// arithmetic of its own — no scaling, no offset, no bound. The address is
    /// whatever the operand holds, so the client is asserting that the word is
    /// readable, and this layer has no way to check that.
    ///
    /// That is why it is NOT how a heap object is read. [`Inst::FieldLoad`]
    /// turns a reference into an address through the region's own addressing
    /// and bounds the slot by the layout, which is the mechanism that makes a
    /// client unable to name a word it does not own. This one has no such
    /// mechanism and cannot grow one: it exists for a word that is not in the
    /// heap at all, whose address the client obtained by asking for it.
    ///
    /// # Why it exists
    ///
    /// A client with a word outside the heap that it reads far more often than
    /// it writes had, until now, exactly one way to read it: a call. A call is
    /// opaque — it clobbers the caller-saved registers and ends every
    /// optimisation the code generator was doing across it — which for a word
    /// read after every operation is most of what the read costs.
    ///
    /// The rejected alternative was a constant address baked in while
    /// compiling. It is wrong for two destinations at once: an object file runs
    /// in a different process from the one that compiled it, and a word that is
    /// per-thread has no single address at all. Taking the address as a VALUE
    /// leaves both of those to the client, which is where they can be answered.
    ///
    /// No safepoint and no barrier: it neither allocates nor stores a
    /// reference, so rule 9's "effects are declared" has nothing to declare.
    WordLoad {
        /// Where to read from, as a machine address.
        address: ValueId,
    },

    /// Reads a field of a registered aggregate.
    ///
    /// Carries no width and no access flags: both come from the layout, so a
    /// caller cannot assert the wrong one.
    FieldLoad {
        /// The aggregate instance.
        object: ValueId,
        /// Its declared type.
        ty: TypeId,
        /// Field index in declaration order.
        field: u32,
    },
    /// Writes a field of a registered aggregate.
    ///
    /// A write to a field the collector traces carries its barrier. The barrier
    /// follows from the field's declared representation and from where the
    /// object lives, never from a flag the caller passes.
    FieldStore {
        /// The aggregate instance.
        object: ValueId,
        /// Its declared type.
        ty: TypeId,
        /// Field index in declaration order.
        field: u32,
        /// The value written.
        value: ValueId,
    },

    /// Allocates an instance of a registered aggregate in a region.
    Alloc {
        /// The type to allocate.
        ty: TypeId,
        /// Where it is placed.
        region: Region,
    },

    /// Parks the frame until something resumes it.
    ///
    /// Produces the value resumption delivers, in the generic form: what a
    /// resumption carries is decided by whoever resumes, and this layer does not
    /// know what that is.
    ///
    /// Only legal in a function whose signature says it may suspend. That is a
    /// property of the function, declared where it is defined, not re-declared at
    /// every point that reaches it.
    Suspend,

    /// Creates a pending promise owned by the current region's scheduler.
    PromiseNew,

    /// Settles a promise, making everything waiting on it runnable.
    PromiseSettle {
        /// The promise being settled.
        promise: ValueId,
        /// What it carries, in the generic form.
        value: ValueId,
        /// Whether this is a failure.
        ///
        /// A rejection resumes its waiter through the unwinding path rather than
        /// delivering a value — one settlement with two outcomes, not two
        /// unrelated mechanisms.
        rejected: bool,
    },

    /// Calls a known function.
    ///
    /// There is no variant for a call that suspends, and no flag for one. Whether
    /// a call parks the caller's frame follows from what the callee is, which the
    /// registry already records — a site that also stated it could state it
    /// wrongly, and the two would then disagree.
    ///
    /// There is likewise no variant for a call behind a shape guard. That is a
    /// guard whose success path contains an ordinary call, which composes with
    /// tail position for free instead of requiring a product of node kinds.
    Call {
        /// The function called.
        callee: FuncId,
        /// Arguments, matching the signature's parameters exactly.
        args: Vec<ValueId>,
    },

    /// Calls a function reached through a value.
    ///
    /// The shape must still be stated: a machine cannot lay out a call it cannot
    /// describe. A client whose callee's arity is genuinely unknown resolves that
    /// before reaching here — the machine does not guess an arity.
    CallIndirect {
        /// The value naming the function.
        callee: ValueId,
        /// The shape being called.
        sig: SigId,
        /// Arguments, matching that shape's parameters exactly.
        args: Vec<ValueId>,
    },

    /// The address of a function, as a value.
    ///
    /// # Why this exists rather than a client writing the address down
    ///
    /// Because the address is not known when the client builds the IR. Code is
    /// placed afterwards, and where it lands is decided by the destination —
    /// executable memory picks one, an object file leaves a relocation for a
    /// linker. A client that wanted a function's address had no way to name one
    /// it had only declared, so this is the missing capability rather than a
    /// convenience.
    ///
    /// # Why `FuncId` and not a symbol name
    ///
    /// [`ConstDecl::Symbol`](crate::ir::ConstDecl) exists and takes a string,
    /// and using it here would have addressed a function this compilation is
    /// building by a name — reintroducing name linkage for the one population
    /// that provably does not need it, since the address is known at emission.
    /// A `FuncId` is what the registry already issues, so a callee that was
    /// never declared is a refusal rather than an unresolved symbol.
    ///
    /// # What it is not
    ///
    /// A callable value. It is a machine address, `Repr::I64`, and what a
    /// client wraps it in — a closure, a method table, a trampoline — is that
    /// client's question. This layer does not know that a function can be a
    /// value in some language, only that its address can be taken.
    FuncAddr {
        /// The function whose address is taken.
        callee: FuncId,
    },

    /// Parks the frame until a promise settles.
    ///
    /// A suspension that names what it waits for. Distinguished from a bare
    /// [`Inst::Suspend`] because the two differ in what resumes them — one is
    /// resumed by whoever holds the frame, the other by a settlement — and a
    /// single node would have to carry an absent operand to express both.
    Await {
        /// The promise waited on.
        promise: ValueId,
    },
}

impl Inst {
    /// The values this instruction reads.
    ///
    /// Every analysis over the IR needs this, and each one deriving it from its
    /// own match over the variants is how an added variant comes to be handled
    /// in three places and forgotten in a fourth.
    pub fn operands(&self) -> Vec<ValueId> {
        match self {
            // `FuncAddr` reads nothing: which function it names is in the
            // instruction, not in a value, which is exactly what makes it
            // constant-foldable and hoistable where an indirect callee is not.
            Inst::Const(_)
            | Inst::Alloc { .. }
            | Inst::Suspend
            | Inst::PromiseNew
            | Inst::FuncAddr { .. } => Vec::new(),

            Inst::Await { promise } => vec![*promise],
            Inst::PromiseSettle { promise, value, .. } => vec![*promise, *value],

            Inst::Call { args, .. } => args.clone(),
            Inst::CallIndirect { callee, args, .. } => {
                let mut operands = vec![*callee];
                operands.extend_from_slice(args);
                operands
            }

            Inst::Widen(v)
            | Inst::Narrow(v, _)
            | Inst::ToInt32(v)
            | Inst::ToF64(v)
            | Inst::FloatUnary(_, v) => vec![*v],

            Inst::IntArith(_, a, b)
            | Inst::FloatArith(_, a, b)
            | Inst::Bitwise(_, a, b)
            | Inst::Compare(_, a, b)
            | Inst::Generic(_, a, b) => vec![*a, *b],

            Inst::WordLoad { address } => vec![*address],

            Inst::FieldLoad { object, .. } => vec![*object],
            Inst::FieldStore { object, value, .. } => vec![*object, *value],
        }
    }

    /// Whether this instruction can trigger a collection.
    ///
    /// Allocation can, and in this design it is the only thing that can:
    /// collection runs from inside the allocator, the allocator is a call, and
    /// every non-tail call is a point where the collector may act. That
    /// correspondence is currently true by accident in the engine this replaces;
    /// stating it here makes it a constraint the layer is designed around rather
    /// than a coincidence it happens to survive.
    pub fn is_safepoint(&self) -> bool {
        matches!(
            self,
            Inst::Alloc { .. } | Inst::Call { .. } | Inst::CallIndirect { .. }
        ) || self.is_suspend()
    }

    /// Whether this instruction parks the frame.
    ///
    /// A suspension is also a safepoint, and for a sharper reason than an
    /// allocation is: a parked frame can sit there across any number of
    /// collections, so what it holds must be findable for as long as it is
    /// parked — not merely at the moment it stops.
    pub fn is_suspend(&self) -> bool {
        matches!(self, Inst::Suspend | Inst::Await { .. })
    }
}

/// A branch target together with the arguments it receives.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BlockCall {
    /// The block control transfers to.
    pub block: BlockId,
    /// Arguments, matching the block's parameter representations exactly.
    pub args: Vec<ValueId>,
}

impl BlockCall {
    /// A transfer to a block that takes no parameters.
    pub fn to(block: BlockId) -> Self {
        Self {
            block,
            args: Vec::new(),
        }
    }
}

/// Why a program stopped.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrapCode {
    /// A point the program proved unreachable was reached.
    Unreachable,
    /// An index fell outside its bounds.
    OutOfBounds,
    /// An integer division by zero.
    DivideByZero,
}

/// How control leaves a block.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Terminator {
    /// Unconditional transfer.
    Jump(BlockCall),
    /// Transfer selected by a proven boolean.
    Branch {
        /// The condition, which must be a proven boolean.
        cond: ValueId,
        /// Taken when the condition holds.
        then_block: BlockCall,
        /// Taken otherwise.
        else_block: BlockCall,
    },
    /// Tests a generic value's representation and narrows it on success.
    ///
    /// This is the only route out of the generic form, and it is a terminator
    /// rather than an instruction so that the failure path cannot be omitted.
    /// The success block receives the narrowed value as its first parameter,
    /// which is what lets the narrowed value exist only where the test held.
    Guard {
        /// The generic value being tested.
        input: ValueId,
        /// The representation the success path assumes.
        expect: Repr,
        /// Entered when the value has that representation; receives it narrowed.
        ok: BlockCall,
        /// Entered otherwise.
        fail: BlockCall,
    },
    /// Returns from the function.
    Return(Vec<ValueId>),

    /// Replaces this frame with a call.
    ///
    /// A terminator rather than an instruction, which is the shape of what it
    /// does: the frame is gone before control transfers, so nothing follows it
    /// and there is no result to bind. Two consequences follow from that and are
    /// checked rather than described. It is not a point where the collector can
    /// act, because there is no frame left to scan. And it cannot be the call a
    /// handler is installed around, because the handler's frame is the frame it
    /// discards — returning a call's result directly and catching that call's
    /// exception are mutually exclusive.
    TailCall {
        /// The function called.
        callee: FuncId,
        /// Arguments, matching the signature's parameters exactly.
        args: Vec<ValueId>,
    },

    /// Replaces this frame with a call through a value.
    TailCallIndirect {
        /// The value naming the function.
        callee: ValueId,
        /// The shape being called.
        sig: SigId,
        /// Arguments, matching that shape's parameters exactly.
        args: Vec<ValueId>,
    },

    /// Tests which type an object is, and narrows it on the success path.
    ///
    /// The companion to [`Terminator::Guard`], and the answer to something that
    /// guard cannot do. The value encoding says a word is a reference and stops
    /// there — proving *which kind* means reading the object, which is what this
    /// does. So the two compose: one guard establishes that a generic value is a
    /// reference, and this one establishes what it refers to.
    ///
    /// Like every guard, the failure path is a required argument rather than an
    /// afterthought: an assumption with nowhere to go when it fails is not an
    /// assumption, it is a hope.
    GuardType {
        /// The object being tested.
        object: ValueId,
        /// The type the success path assumes.
        expect: TypeId,
        /// Entered when it is that type; receives the object, narrowed.
        ok: BlockCall,
        /// Entered otherwise.
        fail: BlockCall,
    },

    /// Reads a property of an object whose layout is not known here.
    ///
    /// The site remembers the last layout it saw and where the property sat in
    /// it. An object of that layout is read without asking anything; one of a
    /// different layout asks once and the site remembers the new answer.
    ///
    /// The remembering is not the client's to write. A site that had to be told
    /// to learn is a site that will be told wrong somewhere, and the failure is
    /// a read that is slow forever without anything being visibly broken.
    ///
    /// Yields a generic value, because what a property holds is not known where
    /// its layout is not. A client that knows the layout has a type guard and an
    /// ordinary field read, which is both faster and more precise — this is for
    /// where it does not.
    /// Writes a property of an object whose layout is not known here.
    ///
    /// The mirror of [`Self::CachedGet`], and it differs in one thing beyond
    /// direction: **there is nothing to hand the success path**. A store
    /// produces no value, so the hit block takes no narrowed parameter and the
    /// two paths meet without one.
    ///
    /// What it does NOT do is add a property. A key the layout does not have
    /// changes what the object is, which is a shape transition — and a
    /// transition is not something a site can remember, because the next object
    /// through it may be at a different layout entirely. The resolver answers
    /// that it cannot be reached this way, and the miss path handles it.
    CachedSet {
        /// The object written.
        object: ValueId,
        /// Which property.
        key: crate::shape::Key,
        /// Where this site keeps what it last saw.
        cache: crate::ir::CacheId,
        /// What to store.
        value: ValueId,
        /// Entered after the store, when the object has the property.
        hit: BlockCall,
        /// Entered when it does not, or holds it somewhere a word does not
        /// reach.
        miss: BlockCall,
    },

    /// Reads a property of an object whose layout is not known here.
    ///
    /// The site remembers the last layout it saw. The remembering is not the
    /// client's to write: a site that had to be told to learn is one that will
    /// be told wrong somewhere, and the failure is a read that is slow forever
    /// without anything being visibly broken.
    ///
    /// Yields a generic value, because what a property holds is not known where
    /// its layout is not. A client that knows the layout has a type guard and an
    /// ordinary field read, which is both faster and more precise — this is for
    /// where it does not.
    CachedGet {
        /// The object read.
        object: ValueId,
        /// Which property.
        key: crate::shape::Key,
        /// Where this site keeps what it last saw.
        cache: crate::ir::CacheId,
        /// Entered with the value, when the object has the property.
        hit: BlockCall,
        /// Entered when it does not, or holds it in a form a word does not hold.
        ///
        /// Required, like every failure path here. What an absent property means
        /// is a question about a client, and a machine that answered it would be
        /// answering for every client at once.
        miss: BlockCall,
    },

    /// Reads at an offset in a cell the site remembers, which need not be the
    /// cell it was asked about.
    ///
    /// # Why this is a second terminator and not a mode of [`Self::CachedGet`]
    ///
    /// Rule 10. It costs one load and one branch more than `CachedGet` on the
    /// recognised path — three loads and two branches when the answer is in
    /// another cell — and a client that only ever reads the cell it asked about
    /// must not pay for the difference. A single operation with a flag would put
    /// that decision at every site, which is the disease this crate exists to
    /// remove.
    ///
    /// # What this layer does and does not know
    ///
    /// It compares two remembered numbers and loads at a remembered offset from
    /// a remembered address. **Which** other cell an answer may live in, and why
    /// one cell's contents would be reachable through another, are questions
    /// about a client — this layer never follows a second step and has no
    /// concept of a chain. The resolver decides; this compares.
    ///
    /// # The two guards, and why both are needed
    ///
    /// The first recognises the object asked about: without it a different
    /// layout would read at an offset computed for this one. The second
    /// recognises the *remembered* cell, because that cell's own layout can
    /// change after this site warmed — and the site holds its address, not a
    /// reference the client could re-resolve.
    ///
    /// The second is emitted **behind** the first and behind a test of whether
    /// there is a second cell at all, so the common answer — the value was in
    /// the cell asked about — pays one load and one predicted branch rather than
    /// three loads and a compare.
    ///
    /// # What the resolver owes this instruction
    ///
    /// The remembered address is not a reference and nothing traces it, so it
    /// must name a cell that cannot be freed while a receiver of the remembered
    /// layout is alive. Stating it here because the instruction cannot enforce
    /// it: this is the one obligation that crosses the boundary in the direction
    /// this crate normally refuses, and it is why the resolver, not this layer,
    /// decides what may be remembered.
    CachedGetIndirect {
        /// The object asked about.
        object: ValueId,
        /// Which property.
        key: crate::shape::Key,
        /// Where this site keeps what it last saw.
        cache: crate::ir::CacheId,
        /// Entered with the value, when both remembered layouts still match.
        hit: BlockCall,
        /// Entered when either does not.
        miss: BlockCall,
    },

    /// Ends a cleanup, handing control back to whatever is unwinding.
    ///
    /// A cleanup is not jumped to. It is copied into each path that needs it,
    /// which is only sound if it is a piece with one entry and one exit — and
    /// this terminator is what makes that structural rather than hoped for.
    ///
    /// A piece, not a block. It may branch and merge inside itself, and may end
    /// this way in more than one of its blocks: several `CleanupDone` are still
    /// one exit, because they all leave to the same place. The single-block
    /// version could hold arithmetic on proven operands and nothing else, which
    /// is not enough for anything a language actually runs on the way out.
    ///
    /// The alternative was a parameter saying where to continue. Rejected: the
    /// representation has no indirect branch to lower it to, and it would make
    /// every cleanup able to reach every continuation, which is an edge in the
    /// graph for every pair and no useful analysis afterwards.
    CleanupDone,

    /// Throws a value.
    ///
    /// The value is in the generic form because this layer does not know what
    /// may be thrown — that is a question about a language. The tag is compared
    /// for equality against handlers and is otherwise not interpreted.
    Throw {
        /// What the value is tagged with.
        tag: Tag,
        /// The value itself, opaque here.
        payload: ValueId,
    },
    /// Stops the program.
    Trap(TrapCode),
}

impl Terminator {
    /// The blocks control may reach from here.
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Jump(call) => vec![call.block],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                vec![then_block.block, else_block.block]
            }
            Terminator::Guard { ok, fail, .. } | Terminator::GuardType { ok, fail, .. } => {
                vec![ok.block, fail.block]
            }
            Terminator::CachedGet { hit, miss, .. }
            | Terminator::CachedGetIndirect { hit, miss, .. }
            | Terminator::CachedSet { hit, miss, .. } => {
                vec![hit.block, miss.block]
            }
            // A throw has no successor in this function's graph. Where it lands
            // is decided by the region tree, and may be in a caller; calling it
            // an edge here would claim a transfer this block does not perform.
            Terminator::Return(_)
            | Terminator::Throw { .. }
            | Terminator::TailCall { .. }
            | Terminator::TailCallIndirect { .. }
            | Terminator::CleanupDone
            | Terminator::Trap(_) => Vec::new(),
        }
    }

    /// The values this terminator reads, including branch arguments.
    pub fn operands(&self) -> Vec<ValueId> {
        let mut operands = Vec::new();
        match self {
            Terminator::Jump(call) => operands.extend_from_slice(&call.args),
            Terminator::Branch {
                cond,
                then_block,
                else_block,
            } => {
                operands.push(*cond);
                operands.extend_from_slice(&then_block.args);
                operands.extend_from_slice(&else_block.args);
            }
            Terminator::Guard {
                input, ok, fail, ..
            } => {
                operands.push(*input);
                operands.extend_from_slice(&ok.args);
                operands.extend_from_slice(&fail.args);
            }
            Terminator::GuardType {
                object, ok, fail, ..
            } => {
                operands.push(*object);
                operands.extend_from_slice(&ok.args);
                operands.extend_from_slice(&fail.args);
            }
            Terminator::CachedGet {
                object, hit, miss, ..
            }
            | Terminator::CachedGetIndirect {
                object, hit, miss, ..
            } => {
                operands.push(*object);
                operands.extend_from_slice(&hit.args);
                operands.extend_from_slice(&miss.args);
            }
            Terminator::CachedSet {
                object,
                value,
                hit,
                miss,
                ..
            } => {
                operands.push(*object);
                operands.push(*value);
                operands.extend_from_slice(&hit.args);
                operands.extend_from_slice(&miss.args);
            }
            Terminator::Return(values) => operands.extend_from_slice(values),
            Terminator::TailCall { args, .. } => operands.extend_from_slice(args),
            Terminator::TailCallIndirect { callee, args, .. } => {
                operands.push(*callee);
                operands.extend_from_slice(args);
            }
            Terminator::Throw { payload, .. } => operands.push(*payload),
            Terminator::CleanupDone | Terminator::Trap(_) => {}
        }
        operands
    }
}

/// An instruction together with the values it defines.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InstData {
    /// The operation.
    pub inst: Inst,
    /// The values it defines, empty for instructions that only have an effect.
    pub results: Vec<ValueId>,
}

impl InstData {
    /// The single value this instruction defines, if it defines exactly one.
    pub fn result(&self) -> Option<ValueId> {
        match self.results.as_slice() {
            [only] => Some(*only),
            _ => None,
        }
    }

    /// Whether this instruction defines the given value.
    pub fn defines(&self, value: ValueId) -> bool {
        self.results.contains(&value)
    }
}

/// A basic block.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct BlockData {
    /// Values the block receives from its predecessors.
    pub params: Vec<ValueId>,
    /// Instructions in order.
    pub insts: Vec<InstId>,
    /// How control leaves. Absent only while the block is being built.
    pub terminator: Option<Terminator>,
}
