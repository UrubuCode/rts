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

/// How a block ends. Exactly one per block, and rule 9's verifier says so.
#[derive(Clone, PartialEq, Debug)]
pub enum Terminator {
    /// To one block, with arguments for its parameters.
    Jump { target: BlockId, args: Vec<ValueId> },
    /// To one of two, on a truth value.
    Branch {
        /// The condition. A language's own notion of truth is a [`Prim`] that
        /// produced this, never something this crate decides.
        condition: ValueId,
        /// Taken when true.
        then_block: BlockId,
        /// Arguments for it.
        then_args: Vec<ValueId>,
        /// Taken when false.
        else_block: BlockId,
        /// Arguments for it.
        else_args: Vec<ValueId>,
    },
    /// Leaving the function.
    Return(Option<ValueId>),
    /// Leaving this tier for the other one, at a paired point.
    ///
    /// Not a call and not an unwind: the generic body of the same function has a
    /// resume label at this `PointId`, in the same binary. README rule 8.
    Fall(PointId),
    /// Ends a cleanup, handing control back to whatever brought us into it.
    ///
    /// # Why a cleanup has one exit and no continuation parameter
    ///
    /// Because it is COPIED into each path that needs it rather than jumped to, and
    /// `rts_cranelift::ir::Terminator::CleanupDone` -- which this is the neutral form
    /// of -- says why the alternative lost: a parameter naming where to continue
    /// *"would make every cleanup able to reach every continuation, which is an edge
    /// in the graph for every pair and no useful analysis afterwards"*, and the
    /// representation has no indirect branch to lower it to anyway.
    ///
    /// A cleanup is a PIECE and not a block. It may branch and merge inside itself,
    /// and more than one of its blocks may end this way: several are still one exit,
    /// because they all leave to the same place.
    ///
    /// This is what makes "one entry, one exit" structural instead of hoped for, and
    /// `verify` refuses it outside a cleanup piece for that reason.
    CleanupDone,
    /// Raising: control leaves along the enclosing region's exception edge.
    ///
    /// # Why it is a terminator and has no successor
    ///
    /// Because an exception edge is not a jump, which is the same thing
    /// [`crate::region`] already says about a handler: nothing jumps to one, so
    /// nothing carries arguments to one, so a handler's predecessors are empty. A
    /// `Raise` naming its handler as a successor would make that false, and every
    /// pass reading the graph as a CFG would then expect an argument list nothing
    /// can supply.
    ///
    /// Where it lands is [`Func::region_of`] over the block it sits in, and out
    /// along [`crate::region::Region::parent`] from there — the search
    /// `rts_cranelift::unwind::plan_unwind` computes, which is why the region tree
    /// is what this carries instead of a target.
    ///
    /// # Why no tag
    ///
    /// A tag says which handlers match, and *what may be thrown* is the one thing
    /// the machine's own header refuses to decide. So the value travels and the
    /// language declares the tag; until it does, the machine refuses this by name
    /// with [`crate::lower::Unlowerable::NeedsHandlerTag`] — the same refusal a
    /// protected region already gets, because it is the same missing declaration.
    Raise(ValueId),
    /// Control does not reach here. A verifier error if it does.
    ///
    /// NOT a raise. This is a trap: the machine's way of saying a point is
    /// unreachable. A language that lowered `throw` to it would get an abort where
    /// the program expects a catchable value.
    Unreachable,
}

impl Terminator {
    /// Every block this one may reach.
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Jump { target, .. } => vec![*target],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => vec![*then_block, *else_block],
            // A RAISE HAS NONE, and that is the claim rather than an omission: see its
            // own doc for why an exception edge is not an edge here.
            Terminator::Return(_)
            | Terminator::Fall(_)
            | Terminator::Raise(_)
            | Terminator::CleanupDone
            | Terminator::Unreachable => Vec::new(),
        }
    }

    /// Every value it reads.
    pub fn reads(&self) -> Vec<ValueId> {
        match self {
            Terminator::Jump { args, .. } => args.clone(),
            Terminator::Branch {
                condition,
                then_args,
                else_args,
                ..
            } => {
                let mut all = vec![*condition];
                all.extend(then_args.iter().copied());
                all.extend(else_args.iter().copied());
                all
            }
            Terminator::Return(Some(value)) => vec![*value],
            Terminator::Raise(value) => vec![*value],
            Terminator::Return(None)
            | Terminator::Fall(_)
            | Terminator::CleanupDone
            | Terminator::Unreachable => Vec::new(),
        }
    }
}

/// A block: its parameters, its instructions, and how it ends.
#[derive(Clone, PartialEq, Debug)]
pub struct Block {
    /// The values its predecessors supply, in order.
    pub params: Vec<ValueId>,
    /// Its instructions, in order.
    pub insts: Vec<InstId>,
    /// How it ends. `None` while it is still being built, which `verify`
    /// refuses.
    pub terminator: Option<Terminator>,
}

/// A function in MIR form.
#[derive(Clone, PartialEq, Debug)]
pub struct Func {
    /// Which tier this is.
    pub tier: Tier,
    /// Whether the frame may be parked in it.
    ///
    /// A property of the FUNCTION and not of the call site, which is the same shape
    /// `rts_cranelift::ir::Signature` gives it and for the same reason: whether a call
    /// parks the caller follows from what the callee is, so a site does not choose it.
    pub may_suspend: bool,
    /// Its blocks. `BlockId(0)` is the entry.
    pub blocks: Vec<Block>,
    /// Its instructions, flat, referenced by the blocks in order.
    pub insts: Vec<Inst>,
    /// How many values it defines.
    pub values: u32,
    /// Its protected regions.
    pub regions: Vec<crate::region::Region>,
    /// Which region each block belongs to, parallel to [`Self::blocks`].
    ///
    /// A parallel vector rather than a field on `Block`, because a pass that rewrites
    /// a block body has no business touching its protection and a field invites it to.
    pub block_regions: Vec<Option<crate::region::RegionId>>,
    /// Every deoptimisation point it declares, in ascending order.
    ///
    /// Held on the function so that `guard::pair` can compare two tiers without
    /// walking either one's blocks — the check runs on every lowering and the
    /// walk would be the expensive part of it.
    pub points: Vec<PointId>,
}

impl Func {
    /// The entry block, which every function has.
    pub fn entry(&self) -> BlockId {
        BlockId(0)
    }

    /// One block.
    pub fn block(&self, block: BlockId) -> &Block {
        &self.blocks[block.0 as usize]
    }

    /// One instruction.
    pub fn inst(&self, inst: InstId) -> &Inst {
        &self.insts[inst.0 as usize]
    }

    /// One region.
    pub fn region(&self, region: crate::region::RegionId) -> &crate::region::Region {
        &self.regions[region.0 as usize]
    }

    /// Which region a block is protected by, if any.
    pub fn region_of(&self, block: BlockId) -> Option<crate::region::RegionId> {
        self.block_regions.get(block.0 as usize).copied().flatten()
    }

    /// Every block, by id.
    pub fn block_ids(&self) -> impl Iterator<Item = BlockId> + use<> {
        (0..self.blocks.len() as u32).map(BlockId)
    }

    /// Which blocks jump to this one.
    ///
    /// Computed rather than stored, because a stored predecessor list is a second
    /// statement of the same fact and the failure mode is a pass that updates the
    /// terminator and not the list.
    pub fn predecessors(&self, block: BlockId) -> Vec<BlockId> {
        self.block_ids()
            .filter(|held| {
                self.block(*held)
                    .terminator
                    .as_ref()
                    .is_some_and(|end| end.successors().contains(&block))
            })
            .collect()
    }

    /// The values an instruction reads.
    pub fn reads(&self, inst: InstId) -> Vec<ValueId> {
        match &self.inst(inst).op {
            Op::Const(_) => Vec::new(),
            Op::Prim { args, .. } => args.clone(),
            Op::Call {
                callee,
                receiver,
                args,
            } => {
                let mut all = match callee {
                    Callee::Dynamic(value) => vec![*value],
                    Callee::Entry(_) | Callee::Func(_) => Vec::new(),
                };
                all.extend(receiver.iter().copied());
                all.extend(args.iter().copied());
                all
            }
            Op::Guard { on, .. } => vec![*on],
            Op::Suspend { value } => value.iter().copied().collect(),
        }
    }
}

/// Builds one function, minting values and blocks.
///
/// # Why a builder rather than public fields
///
/// Because SSA's one property — a value defined exactly once — is the kind of
/// invariant that a caller assembling `Vec`s by hand breaks in a way that
/// compiles. The builder mints every value, so defining one twice is not
/// expressible; `verify` then checks what the builder cannot, which is order.
pub struct FuncBuilder {
    func: Func,
    current: BlockId,
    /// The regions open where building is, innermost last.
    open: Vec<crate::region::RegionId>,
}

impl FuncBuilder {
    /// A function with an empty entry block.
    pub fn new(tier: Tier) -> Self {
        Self {
            func: Func {
                tier,
                may_suspend: false,
                blocks: vec![Block {
                    params: Vec::new(),
                    insts: Vec::new(),
                    terminator: None,
                }],
                insts: Vec::new(),
                values: 0,
                regions: Vec::new(),
                block_regions: vec![None],
                points: Vec::new(),
            },
            current: BlockId(0),
            open: Vec::new(),
        }
    }

    /// A new, empty block.
    pub fn block(&mut self) -> BlockId {
        self.func.blocks.push(Block {
            params: Vec::new(),
            insts: Vec::new(),
            terminator: None,
        });
        // A block made while a region is open is INSIDE it. Anything else would make
        // protection depend on the order a lowering happens to create blocks in.
        self.func.block_regions.push(self.open.last().copied());
        BlockId(self.func.blocks.len() as u32 - 1)
    }

    /// Where instructions are appended.
    pub fn switch_to(&mut self, block: BlockId) {
        self.current = block;
    }

    /// The block being appended to.
    pub fn current(&self) -> BlockId {
        self.current
    }

    /// A parameter of a block, which its predecessors supply.
    pub fn param(&mut self, block: BlockId) -> ValueId {
        let value = self.mint();
        self.func.blocks[block.0 as usize].params.push(value);
        value
    }

    /// Opens a protected region, which every block made until it closes belongs to.
    ///
    /// The block being built joins it too, and that is not an accident: a raise emitted
    /// before any new block is created would otherwise be planned as if it were
    /// outside, and "the first statement of a `try`" is not a corner case.
    /// `rts_cranelift::ir::FuncBuilder::open_region` states the same thing about its
    /// own regions, which is where this discipline comes from.
    pub fn open_region(
        &mut self,
        handler: Option<BlockId>,
        cleanup: Option<BlockId>,
    ) -> crate::region::RegionId {
        let parent = self.open.last().copied();
        self.func.regions.push(crate::region::Region {
            parent,
            handler,
            cleanup,
        });
        let region = crate::region::RegionId(self.func.regions.len() as u32 - 1);
        let held = self.current;
        self.func.block_regions[held.0 as usize] = Some(region);
        self.open.push(region);
        region
    }

    /// Closes the innermost open region.
    ///
    /// Takes no argument for the reason the machine's own does not: a client that could
    /// name a region could name one that does not enclose the block it is building.
    pub fn close_region(&mut self) {
        self.open.pop();
    }

    /// Which tier this builder is building.
    ///
    /// Asked rather than remembered by the client, because the one decision that
    /// turns on it -- whether a guard may exist at all -- is taken in a different
    /// crate from the one that chose the tier. A client keeping its own copy is two
    /// places for the answer, and the drift is a guard with nowhere to fall.
    pub fn tier(&self) -> Tier {
        self.func.tier
    }

    /// The entry block, for a client that switched away from it.
    pub fn entry_block(&self) -> BlockId {
        BlockId(0)
    }

    /// Appends an instruction to the current block and answers its result.
    pub fn push(&mut self, op: Op, effect: Effect, at: Position) -> ValueId {
        let result = self.mint();
        if let Op::Guard { point, .. } = &op {
            self.declare(*point);
        }
        // THE FUNCTION'S FLAG IS DERIVED AND NEVER PASSED IN. A builder that asked
        // its client to set `may_suspend` as well as to push the suspension would be
        // holding one fact in two places, and the two would drift the first time a
        // lowering grew a path it forgot to mark -- which is a function the machine
        // would compile with an ordinary frame and then park.
        //
        // Read from the EFFECT rather than from the operation, because that is where
        // the fact lives: a language that parks inside a primitive of its own says so
        // in its effect table, and this stays true without naming its primitives.
        if effect.may_suspend() {
            self.func.may_suspend = true;
        }
        self.func.insts.push(Inst {
            op,
            result,
            at,
            effect,
        });
        let inst = InstId(self.func.insts.len() as u32 - 1);
        self.func.blocks[self.current.0 as usize].insts.push(inst);
        result
    }

    /// Ends the current block.
    ///
    /// Terminating one twice is refused rather than overwritten: the second call
    /// is a bug in the lowering, and silently keeping one of the two would make
    /// which one arbitrary.
    pub fn end(&mut self, terminator: Terminator) {
        if let Terminator::Fall(point) = &terminator {
            self.declare(*point);
        }
        let block = &mut self.func.blocks[self.current.0 as usize];
        assert!(
            block.terminator.is_none(),
            "a block was terminated twice, which makes which terminator survives arbitrary"
        );
        block.terminator = Some(terminator);
    }

    /// Declares a point this body can be resumed at, emitting nothing.
    ///
    /// # Why the generic tier needs this and the specialised one does not
    ///
    /// Because the specialised body declares a point by EMITTING the guard that falls
    /// to it, and the generic body emits no guard at all -- it is where a fall lands.
    /// Without this it declares no points, so `guard::pair` answers `Unresumable` for
    /// every function that speculates about anything: the specialised tier falls to a
    /// point the generic tier never claimed.
    ///
    /// That was live and unnoticed, and it is exactly what `pair`'s own doc predicted
    /// about itself: *"a property nothing checks is a property nobody finds out about"*.
    pub fn resumable(&mut self, point: PointId) {
        self.declare(point);
    }

    /// The finished function. `verify` is what says it is well formed.
    pub fn finish(self) -> Func {
        self.func
    }

    fn mint(&mut self) -> ValueId {
        let value = ValueId(self.func.values);
        self.func.values += 1;
        value
    }

    fn declare(&mut self, point: PointId) {
        if let Err(at) = self.func.points.binary_search(&point) {
            self.func.points.insert(at, point);
        }
    }
}
