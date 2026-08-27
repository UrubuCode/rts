//! Rewriting a suspending function into a resumable one.
//!
//! # The problem, and why the obvious answer is wrong
//!
//! A value defined before a suspension and used after it cannot stay in a
//! register: the frame is gone while it is parked, and when it resumes the
//! machine is entered afresh with nothing in it. So the value has to be written
//! down somewhere and read back.
//!
//! The tempting shape is to reload at the resume point and rewrite the uses
//! after it. That requires reconstructing the value's definition for every place
//! that reads it — full reconstruction over arbitrary control flow, because a
//! block after the suspension may also be reachable from before it, and then a
//! reload would read a slot that was never written.
//!
//! # What this does instead
//!
//! **A value that outlives a suspension does not live in a register at all.** It
//! is stored where it is defined and loaded where it is used. There is no
//! definition to reconstruct, because the value never crosses the boundary in
//! the first place — it is already in the frame before anything can suspend.
//!
//! The cost is a store per definition and a load per use, on exactly the values
//! that had to be in memory regardless. Everything else stays in registers
//! untouched. And the loads sit next to their uses within a block, which is the
//! shape an optimizer removes.
//!
//! # What the resumable form looks like
//!
//! One argument, the frame; one result, whether it finished. Parameters and
//! returns live in the frame, because a resumed frame is entered afresh and
//! anything it still needs must have been written down. Entering runs a dispatch
//! on the resume label: zero means start from the beginning, and each suspension
//! point has a number that leads back to just after it.

use std::collections::HashMap;

use super::layout::FrameLayout;
use super::{ResumeLabel, ResumeMode, SuspendPlan, plan_suspension};
use crate::ir::{
    BlockCall, BlockId, CmpOp, ConstDecl, Function, Inst, ScalarBits, Signature, Terminator,
    ValueId,
};
use crate::repr::{RefKind, Repr};
use crate::types::TypeRegistry;
use crate::unwind::{Handler, RegionId, Tag};

/// A function rewritten so that it can be parked and picked back up.
pub struct Resumable {
    /// The rewritten function. Contains no suspension.
    pub func: Function,
    /// Where everything it needs lives while it is parked.
    pub layout: FrameLayout,
    /// How many resume labels the dispatch distinguishes.
    ///
    /// Zero is "not started"; each suspension point has one.
    pub resume_points: usize,
}

/// Why a function could not be rewritten.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TransformError {
    /// The function does not declare that it may park its frame.
    ///
    /// Rewriting one that does not would change what it is without anything
    /// having said so.
    NotSuspending,
    /// A suspension appears somewhere the rewrite cannot split.
    ///
    /// A suspension has to become the end of a block, so that resuming can enter
    /// what follows. Reaching this means one was found where a block cannot be
    /// split — which the representation does not currently allow, and which is
    /// worth reporting rather than silently producing a frame that resumes into
    /// the wrong place.
    UnsplittableSuspension {
        /// The block it is in.
        block: BlockId,
    },
}

/// Rewrites a suspending function into a resumable one.
///
/// `abrupt` is the tag a resumption of [`ResumeMode::Unwind`] leaves with. It
/// is a parameter rather than a constant of this crate because a tag means
/// whatever the client that handles it says it means — rule 2: this layer
/// compares tags and does not interpret them, so it cannot choose one. Passing
/// the wrong tag is a resumption whose payload no handler in the client's
/// program matches, which is why it is required rather than defaulted.
pub fn resumable_form(
    func: &Function,
    types: &mut TypeRegistry,
    abrupt: Tag,
) -> Result<Resumable, TransformError> {
    if !func.signature.may_suspend {
        return Err(TransformError::NotSuspending);
    }

    let plan = plan_suspension(func);
    let layout = FrameLayout::declare(func, &plan, types);
    Rewrite::new(func, &plan, layout, abrupt).run()
}

/// Whether anything that PARKS a frame survives the rewrite.
///
/// `Suspend` does not — that is what the rewrite is for, and a resume label
/// takes its place. `Await` does, because parking on a promise is not a yield
/// and the runtime resumes it through a different door. So an `async
/// function*` still suspends after being made resumable, and a plain
/// `function*` no longer does.
fn source_awaits(source: &Function) -> bool {
    source
        .blocks()
        .flat_map(|(_, data)| data.insts.iter().copied())
        .filter_map(|inst| source.inst(inst))
        .any(|data| matches!(data.inst, Inst::Await { .. }))
}

/// One rewriting in progress.
struct Rewrite<'a> {
    source: &'a Function,
    plan: &'a SuspendPlan,
    layout: FrameLayout,
    out: Function,
    frame: ValueId,
    /// Where each of the source's blocks begins in the rewritten function.
    blocks: HashMap<BlockId, BlockId>,
    /// Where each resume label continues.
    resume_targets: HashMap<ResumeLabel, BlockId>,
    /// What a source value became, for values that stay in registers.
    values: HashMap<ValueId, ValueId>,
    /// What a resumption that unwinds leaves with.
    abrupt: Tag,
}

impl<'a> Rewrite<'a> {
    fn new(
        source: &'a Function,
        plan: &'a SuspendPlan,
        layout: FrameLayout,
        abrupt: Tag,
    ) -> Self {
        let out = Function::new(Signature {
            params: vec![Repr::Ref(RefKind::Aggregate(layout.ty))],
            returns: vec![Repr::Bool],
            // The rewrite removes `Suspend` and keeps `Await`, so whether the
            // REWRITTEN form may suspend is a different question from whether
            // the source could — and it is answered from what survives rather
            // than inherited or defaulted.
            //
            // Defaulted, it was `false`, and that was invisible for as long as
            // the only suspending functions rewritten here were plain
            // generators: a `function*` has no `Await` to leave behind. An
            // `async function*` has both, and the first one written refused the
            // whole program with `UndeclaredSuspension` — the verifier saying
            // that a body which can park declared it could not.
            may_suspend: source_awaits(source),
            ..Signature::default()
        });
        let frame = out.block(out.entry).expect("entry exists").params[0];

        // A cache site is numbered per function, exactly as a constant is, and
        // the rewritten function starts with none — so an instruction naming
        // site 0 named a site the new function had never declared, and the
        // verifier refused it as `UnknownCache`. Declaring as many as the source
        // had keeps every number meaning what it meant; the sites are empty at
        // run time either way, since what they remember is filled in by running.
        let mut out = out;
        for _ in 0..source.cache_count() {
            out.push_cache();
        }

        Self {
            source,
            plan,
            layout,
            out,
            frame,
            blocks: HashMap::new(),
            resume_targets: HashMap::new(),
            values: HashMap::new(),
            abrupt,
        }
    }

    fn run(mut self) -> Result<Resumable, TransformError> {
        self.create_blocks();
        self.copy_regions();
        self.rewrite_blocks()?;
        self.emit_dispatch();

        Ok(Resumable {
            func: self.out,
            layout: self.layout,
            resume_points: self.plan.points.len(),
        })
    }

    /// Makes a home for every source block, keeping its parameters.
    ///
    /// Parameters are kept even for values that live in the frame: the edge that
    /// passes one still exists, and storing it on arrival is cheaper than
    /// rewriting every edge to stop passing it.
    fn create_blocks(&mut self) {
        for (id, data) in self.source.blocks() {
            let block = self.out.push_block();

            // The source's entry block keeps no parameters: they were the
            // function's own, they live in the frame now, and a block that still
            // declared them would have to be jumped to with values the dispatch
            // does not have.
            if id != self.source.entry {
                for &param in &data.params {
                    let repr = self.source.repr_of(param);
                    let value = self.out.push_block_param(block, repr);
                    self.values.insert(param, value);
                }
            }
            self.blocks.insert(id, block);
        }
    }

    /// Carries the protected regions across, and puts each block back in its own.
    ///
    /// The fourth thing a `Function` owns beside its blocks, after its constant
    /// pool, its cache sites and the order they are read in. Without it a body
    /// with a `finally` in it produced a cleanup block that belonged to no
    /// region, so its `CleanupEnd` was refused as `CleanupEndOutsideCleanup` —
    /// a rewritten function that only the rewriter believed in.
    ///
    /// A region is declared in source order, so a parent is declared before its
    /// children here exactly as it was there, and the identifiers line up
    /// one-to-one. What is remapped is every BLOCK a region names: a handler's
    /// entry and the cleanup.
    fn copy_regions(&mut self) {
        for index in 0..self.source.regions.len() {
            let id = RegionId(index as u32);
            let region = self
                .source
                .regions
                .get(id)
                .expect("index below the count is a declared region");
            let handlers = region
                .handlers
                .iter()
                .map(|handler| Handler {
                    tag: handler.tag,
                    block: self.blocks[&handler.block],
                })
                .collect();
            let cleanup = region.cleanup.map(|block| self.blocks[&block]);
            let resume_return = region.resume_return.map(|block| self.blocks[&block]);
            let id = self.out.regions.declare(region.parent, handlers, cleanup);
            if let Some(block) = resume_return {
                self.out.regions.set_resume_return(id, block);
            }
        }

        for (id, _) in self.source.blocks() {
            if let Some(region) = self.source.region_of(id) {
                self.out.set_block_region(self.blocks[&id], region);
            }
        }
    }

    fn rewrite_blocks(&mut self) -> Result<(), TransformError> {
        for (source_block, data) in self.source.blocks().map(|(id, d)| (id, d.clone())) {
            let mut current = self.blocks[&source_block];

            // A parameter that outlives a suspension is written down on arrival,
            // so that everything after reads it from one place. The entry block's
            // are already in the frame — that is where they came from.
            if source_block != self.source.entry {
                for &param in &data.params {
                    self.store_if_spilled(current, param);
                }
            }

            for &inst_id in &data.insts {
                let inst = self
                    .source
                    .inst(inst_id)
                    .expect("instruction belongs to the source");

                if matches!(inst.inst, Inst::Suspend) {
                    current = self.split_at_suspension(current, inst_id, &inst.results)?;
                    continue;
                }

                let rewritten = self.rewrite_inst(current, &inst.inst);
                let reprs: Vec<_> = inst
                    .results
                    .iter()
                    .map(|&r| self.source.repr_of(r))
                    .collect();
                let produced = self.out.push_inst(current, rewritten, &reprs);

                for (&source_value, &new_value) in inst.results.iter().zip(&produced) {
                    self.values.insert(source_value, new_value);
                    self.store_if_spilled(current, source_value);
                }
            }

            let terminator =
                data.terminator
                    .clone()
                    .ok_or(TransformError::UnsplittableSuspension {
                        block: source_block,
                    })?;
            self.rewrite_terminator(current, &terminator);
        }
        Ok(())
    }

    /// Ends the current block at a suspension and opens the one that resumes.
    fn split_at_suspension(
        &mut self,
        current: BlockId,
        inst: crate::ir::InstId,
        results: &[ValueId],
    ) -> Result<BlockId, TransformError> {
        let label = self
            .plan
            .label_of(inst)
            .ok_or(TransformError::UnsplittableSuspension { block: current })?;

        // Say where to come back to, then say that this did not finish.
        let label_value = self.constant(current, Repr::I64, u64::from(label.0) + 1);
        self.store_field(current, self.layout.label_field, label_value);

        // The `Return` that PARKS goes in a block of its own, outside every
        // protected region, and the suspension jumps to it.
        //
        // Because lowering runs a region's cleanups on any `Return` inside it —
        // "leaving a region normally owes the same cleanup as leaving it by
        // throwing", which is right for a return and wrong for this one. Parking
        // is not leaving: the frame comes back to the very next instruction. With
        // the `Return` left in the region, a `try { yield 1; yield 2; } finally
        // { … }` ran its `finally` THREE times — once per yield and once on the
        // way out — which is what `claude-generator-variavel-externa` measured.
        let parking = self.out.push_block();
        self.out
            .set_terminator(current, Terminator::Jump(BlockCall::to(parking)));
        let unfinished = self.constant(parking, Repr::Bool, 0);
        self.out
            .set_terminator(parking, Terminator::Return(vec![unfinished]));

        // Resuming lands back INSIDE whatever the suspension was inside. The
        // block was created without a region, so the statements after a `yield`
        // in a `try` were outside the `try` they were written in — the same bug
        // from the other end, and invisible until something threw there.
        let region = self.out.region_of(current);
        let resumed = self.block_in(region);
        self.resume_targets.insert(label, resumed);

        // Not every resumption carries on. See [`ResumeMode`].
        let delivered = self.dispatch_on_mode(resumed, region);

        // Whoever resumes leaves the value in the frame; the frame reads it.
        if let Some(&result) = results.first() {
            let repr = self.source.repr_of(result);
            let loaded = self.load_field(delivered, self.layout.resumed_field, repr);
            self.values.insert(result, loaded);
            self.store_if_spilled(delivered, result);
        }
        Ok(delivered)
    }

    /// Splits a resumption on how it was made, and answers where it carries on.
    ///
    /// Three ways out of one point, and each is expressed with what the region
    /// tree already does at that point rather than with anything new:
    ///
    /// - carrying on is the block this answers, which is where the suspension's
    ///   result is read and the body continues;
    /// - unwinding is a `Throw` written AT the suspension, so the handler search
    ///   starts from the regions the suspension was inside — which is the whole
    ///   reason this is a mode of resuming rather than something the resumer
    ///   could do from outside, where those regions are no longer entered;
    /// - returning is a `Return` written at the same place, so lowering runs
    ///   what leaving those regions owes on the way out.
    ///
    /// Every block is put in the suspension's own region for that reason. They
    /// are all dominated by the resume target, so nothing that dominated the
    /// region before dominates less of it now — which is what the cleanup rule
    /// reads and what a new block in a protected region could otherwise break.
    fn dispatch_on_mode(&mut self, resumed: BlockId, region: Option<RegionId>) -> BlockId {
        let deliver = self.block_in(region);
        let abrupt = self.block_in(region);

        // Read once and used in both blocks: `abrupt` has one predecessor and
        // it is `resumed`, so the definition dominates every use of it.
        let mode = self.load_field(resumed, self.layout.mode_field, Repr::I64);
        let ordinary = self.constant(resumed, Repr::I64, ResumeMode::Deliver.number());
        let is_ordinary = self.compare(resumed, mode, ordinary);
        self.out.set_terminator(
            resumed,
            Terminator::Branch {
                cond: is_ordinary,
                then_block: BlockCall::to(deliver),
                else_block: BlockCall::to(abrupt),
            },
        );

        let unwinding = self.block_in(region);
        let returning = self.block_in(region);
        let raising = self.constant(abrupt, Repr::I64, ResumeMode::Unwind.number());
        let is_raising = self.compare(abrupt, mode, raising);
        self.out.set_terminator(
            abrupt,
            Terminator::Branch {
                cond: is_raising,
                then_block: BlockCall::to(unwinding),
                else_block: BlockCall::to(returning),
            },
        );

        // The delivered value is the payload. It is read at its own
        // representation, which is the record's rather than the suspension
        // result's: a `Throw` carries a generic value and the field holds one.
        let payload = self.load_field(unwinding, self.layout.resumed_field, Repr::Tagged);
        self.out.set_terminator(
            unwinding,
            Terminator::Throw {
                tag: self.abrupt,
                payload,
            },
        );

        // Returning is the fall-through of the chain, so a number that is
        // neither of the other two lands here rather than anywhere undefined —
        // rule 12, and this is the safe end of the three: it leaves the frame
        // once, with nothing invented, where unwinding with a payload nobody
        // wrote would be a throw of whatever the slot happened to hold.
        //
        // Nothing is written where the function's answer goes. The resumer
        // named the value and still holds it, and the frame's return slot need
        // not even be the representation the delivered value is — see
        // [`ResumeMode::Return`]. What this owes is that the regions between
        // here and the exit are left properly, which is what a `Return` in this
        // block means and what lowering does with it.
        //
        // Unless a region between here and the exit said where such a
        // resumption carries on. A cleanup cannot be one that PARKS — see
        // [`crate::unwind::Region::resume_return`] — so a region holding one
        // names an ordinary block instead, and the `Return` written here would
        // walk straight past it.
        match self.returns_through(region) {
            Some(target) => {
                let held = self.load_field(returning, self.layout.resumed_field, Repr::Tagged);
                self.out.set_terminator(
                    returning,
                    Terminator::Jump(BlockCall {
                        block: target,
                        args: vec![held],
                    }),
                );
            }
            None => {
                let finished = self.constant(returning, Repr::Bool, 1);
                self.out
                    .set_terminator(returning, Terminator::Return(vec![finished]));
            }
        }

        deliver
    }

    /// Where a resumption that returns from inside `region` carries on.
    ///
    /// The innermost enclosing region that named one, which is the same search
    /// order a thrown value's handler is found in — and it has to be, because
    /// the two are the same nesting seen from two exits.
    fn returns_through(&self, region: Option<RegionId>) -> Option<BlockId> {
        self.source
            .regions
            .enclosing(region?)
            .find_map(|(_, held)| held.resume_return)
            .map(|block| self.blocks[&block])
    }

    /// A fresh block, inside the region the one it continues was inside.
    fn block_in(&mut self, region: Option<RegionId>) -> BlockId {
        let block = self.out.push_block();
        if let Some(region) = region {
            self.out.set_block_region(block, region);
        }
        block
    }

    /// Compares two proven values for equality.
    fn compare(&mut self, block: BlockId, left: ValueId, right: ValueId) -> ValueId {
        self.out
            .push_inst(block, Inst::Compare(CmpOp::Eq, left, right), &[Repr::Bool])
            .pop()
            .expect("a comparison defines one value")
    }

    /// Rewrites one instruction's operands into the rewritten function's values.
    fn rewrite_inst(&mut self, block: BlockId, inst: &Inst) -> Inst {
        // A constant is named by its position in the function's own pool, and
        // the rewritten function has a different pool — this one already holds
        // the labels and the finished flags the rewrite itself declared. Cloning
        // the instruction alone carried the source's NUMBER into a pool where it
        // names something else, so a body yielding the double `1` reached the
        // machine as the label `1`, and the widening emitted for a double was
        // applied to an integer. The verifier caught it; nothing before this
        // could, because the transform's own tests never used a constant from
        // the source.
        if let Inst::Const(id) = inst {
            let decl = self
                .source
                .constant(*id)
                .expect("a constant belongs to the function that names it")
                .clone();
            return Inst::Const(self.out.push_const(decl));
        }

        let mut rewritten = inst.clone();
        let operands = inst.operands();
        let replacements: Vec<_> = operands
            .iter()
            .map(|&value| self.use_value(block, value))
            .collect();
        replace_operands(&mut rewritten, &replacements);
        rewritten
    }

    fn rewrite_terminator(&mut self, block: BlockId, terminator: &Terminator) {
        let mut rewritten = terminator.clone();

        match &mut rewritten {
            // Returning means finishing: the values go into the frame, and the
            // answer is that there is nothing left to come back to.
            Terminator::Return(values) => {
                let stored: Vec<_> = values
                    .iter()
                    .map(|&value| self.use_value(block, value))
                    .collect();
                for (position, value) in stored.into_iter().enumerate() {
                    let field = self.layout.return_fields[position];
                    self.store_field(block, field, value);
                }
                let finished = self.constant(block, Repr::Bool, 1);
                self.out
                    .set_terminator(block, Terminator::Return(vec![finished]));
                return;
            }

            Terminator::Jump(call) => self.rewrite_block_call(block, call),
            Terminator::Branch {
                cond,
                then_block,
                else_block,
            } => {
                *cond = self.use_value(block, *cond);
                self.rewrite_block_call(block, then_block);
                self.rewrite_block_call(block, else_block);
            }
            Terminator::Guard {
                input, ok, fail, ..
            } => {
                *input = self.use_value(block, *input);
                self.rewrite_block_call(block, ok);
                self.rewrite_block_call(block, fail);
            }
            Terminator::GuardType {
                object, ok, fail, ..
            } => {
                *object = self.use_value(block, *object);
                self.rewrite_block_call(block, ok);
                self.rewrite_block_call(block, fail);
            }
            // Both cached reads, and the indirect one is here for the reason the
            // direct one is: what it rewrites is the RECEIVER, which is an
            // ordinary value that a park may have spilled. The address the
            // indirect form remembers is not an operand and not a value — it
            // lives in the site's cell — so there is nothing of it to rewrite,
            // and a park cannot move it.
            Terminator::CachedGet {
                object, hit, miss, ..
            }
            | Terminator::CachedGetIndirect {
                object, hit, miss, ..
            } => {
                *object = self.use_value(block, *object);
                self.rewrite_block_call(block, hit);
                self.rewrite_block_call(block, miss);
            }
            // The keyed form rewrites BOTH, and that is the whole difference
            // from the two above: its key is an ordinary value a park may have
            // spilled, where theirs is a number fixed while compiling and has
            // nothing in a frame to move.
            Terminator::CachedGetKeyed {
                object,
                key,
                hit,
                miss,
                ..
            } => {
                *object = self.use_value(block, *object);
                *key = self.use_value(block, *key);
                self.rewrite_block_call(block, hit);
                self.rewrite_block_call(block, miss);
            }
            Terminator::CachedSet {
                object,
                value,
                hit,
                miss,
                ..
            } => {
                // Both operands, and the value is the half a read has no
                // equivalent of: what is stored is live across the store and
                // has to come out of the frame like everything else.
                *object = self.use_value(block, *object);
                *value = self.use_value(block, *value);
                self.rewrite_block_call(block, hit);
                self.rewrite_block_call(block, miss);
            }
            Terminator::Throw { payload, .. } => {
                *payload = self.use_value(block, *payload);
            }
            Terminator::TailCall { args, .. } => {
                *args = args.iter().map(|&a| self.use_value(block, a)).collect();
            }
            Terminator::TailCallIndirect { callee, args, .. } => {
                *callee = self.use_value(block, *callee);
                *args = args.iter().map(|&a| self.use_value(block, a)).collect();
            }
            Terminator::CleanupDone | Terminator::Trap(_) => {}
        }

        self.out.set_terminator(block, rewritten);
    }

    fn rewrite_block_call(&mut self, block: BlockId, call: &mut BlockCall) {
        call.block = self.blocks[&call.block];
        call.args = call
            .args
            .iter()
            .map(|&arg| self.use_value(block, arg))
            .collect();
    }

    /// Reads a source value, from the frame if it lives there.
    fn use_value(&mut self, block: BlockId, value: ValueId) -> ValueId {
        match self.layout.slot_of(value) {
            Some(field) => {
                let repr = self.source.repr_of(value);
                self.load_field(block, field, repr)
            }
            None => self.values[&value],
        }
    }

    /// Writes a value down, if it has to survive a suspension.
    fn store_if_spilled(&mut self, block: BlockId, value: ValueId) {
        if let Some(field) = self.layout.slot_of(value) {
            let stored = self.values[&value];
            self.store_field(block, field, stored);
        }
    }

    fn load_field(&mut self, block: BlockId, field: u32, repr: Repr) -> ValueId {
        let inst = Inst::FieldLoad {
            object: self.frame,
            ty: self.layout.ty,
            field,
        };
        self.out
            .push_inst(block, inst, &[repr])
            .pop()
            .expect("a load defines one value")
    }

    fn store_field(&mut self, block: BlockId, field: u32, value: ValueId) {
        let inst = Inst::FieldStore {
            object: self.frame,
            ty: self.layout.ty,
            field,
            value,
        };
        self.out.push_inst(block, inst, &[]);
    }

    fn constant(&mut self, block: BlockId, repr: Repr, bits: u64) -> ValueId {
        let decl = self.out.push_const(ConstDecl::Scalar {
            repr,
            bits: ScalarBits(bits),
        });
        self.out
            .push_inst(block, Inst::Const(decl), &[repr])
            .pop()
            .expect("a constant defines one value")
    }

    /// Builds the entry dispatch: read the label, go where it says.
    ///
    /// A chain of comparisons rather than a jump table, because the
    /// representation has no table and the counts here are small — a function
    /// with enough suspension points for the difference to matter would want a
    /// table in the representation, which is a change to make when something
    /// needs it rather than in advance.
    fn emit_dispatch(&mut self) {
        let entry = self.out.entry;
        let start = self.blocks[&self.source.entry];

        let label = self.load_field(entry, self.layout.label_field, Repr::I64);
        let mut current = entry;

        let targets: Vec<_> = self
            .plan
            .points
            .iter()
            .filter_map(|(_, label)| self.resume_targets.get(label).map(|&b| (*label, b)))
            .collect();

        for (resume_label, target) in targets {
            let expected = self.constant(current, Repr::I64, u64::from(resume_label.0) + 1);
            let matches = self.compare(current, label, expected);

            let next = self.out.push_block();
            self.out.set_terminator(
                current,
                Terminator::Branch {
                    cond: matches,
                    then_block: BlockCall::to(target),
                    else_block: BlockCall::to(next),
                },
            );
            current = next;
        }

        // No label matched, so this frame has not started.
        self.out
            .set_terminator(current, Terminator::Jump(BlockCall::to(start)));
    }
}

/// Puts rewritten operands back into an instruction, in the order they were read.
fn replace_operands(inst: &mut Inst, replacements: &[ValueId]) {
    let mut next = replacements.iter().copied();
    let mut take = || next.next().expect("one replacement per operand");

    match inst {
        Inst::Const(_)
        | Inst::Alloc { .. }
        | Inst::Suspend
        | Inst::PromiseNew
        | Inst::FuncAddr { .. } => {}

        Inst::Widen(v)
        | Inst::ToInt32(v)
        | Inst::ToF64(v)
        | Inst::ToF64Unsigned(v)
        | Inst::WordLoad { address: v }
        | Inst::IsSingleton { value: v, .. }
        | Inst::FloatUnary(_, v) => *v = take(),

        Inst::IntArith(_, a, b)
        | Inst::FloatArith(_, a, b)
        | Inst::Bitwise(_, a, b)
        | Inst::Compare(_, a, b)
        | Inst::Generic(_, a, b) => {
            *a = take();
            *b = take();
        }

        Inst::ElementLoad {
            base,
            index,
            length,
        } => {
            *base = take();
            *index = take();
            *length = take();
        }

        Inst::FieldLoad { object, .. } => *object = take(),
        Inst::FieldStore { object, value, .. } => {
            *object = take();
            *value = take();
        }

        Inst::Await { promise } => *promise = take(),
        Inst::PromiseSettle { promise, value, .. } => {
            *promise = take();
            *value = take();
        }

        Inst::Call { args, .. } => {
            for arg in args {
                *arg = take();
            }
        }
        Inst::CallIndirect { callee, args, .. } => {
            *callee = take();
            for arg in args {
                *arg = take();
            }
        }
    }
}
