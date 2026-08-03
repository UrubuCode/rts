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
use super::{ResumeLabel, SuspendPlan, plan_suspension};
use crate::ir::{
    BlockCall, BlockId, CmpOp, ConstDecl, Function, Inst, ScalarBits, Signature, Terminator,
    ValueId,
};
use crate::repr::{RefKind, Repr};
use crate::types::TypeRegistry;

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
pub fn resumable_form(
    func: &Function,
    types: &mut TypeRegistry,
) -> Result<Resumable, TransformError> {
    if !func.signature.may_suspend {
        return Err(TransformError::NotSuspending);
    }

    let plan = plan_suspension(func);
    let layout = FrameLayout::declare(func, &plan, types);
    Rewrite::new(func, &plan, layout).run()
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
}

impl<'a> Rewrite<'a> {
    fn new(source: &'a Function, plan: &'a SuspendPlan, layout: FrameLayout) -> Self {
        let out = Function::new(Signature {
            params: vec![Repr::Ref(RefKind::Aggregate(layout.ty))],
            returns: vec![Repr::Bool],
            ..Signature::default()
        });
        let frame = out.block(out.entry).expect("entry exists").params[0];

        Self {
            source,
            plan,
            layout,
            out,
            frame,
            blocks: HashMap::new(),
            resume_targets: HashMap::new(),
            values: HashMap::new(),
        }
    }

    fn run(mut self) -> Result<Resumable, TransformError> {
        self.create_blocks();
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
        let unfinished = self.constant(current, Repr::Bool, 0);
        self.out
            .set_terminator(current, Terminator::Return(vec![unfinished]));

        let resumed = self.out.push_block();
        self.resume_targets.insert(label, resumed);

        // Whoever resumes leaves the value in the frame; the frame reads it.
        if let Some(&result) = results.first() {
            let repr = self.source.repr_of(result);
            let loaded = self.load_field(resumed, self.layout.resumed_field, repr);
            self.values.insert(result, loaded);
            self.store_if_spilled(resumed, result);
        }
        Ok(resumed)
    }

    /// Rewrites one instruction's operands into the rewritten function's values.
    fn rewrite_inst(&mut self, block: BlockId, inst: &Inst) -> Inst {
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
            let matches = self
                .out
                .push_inst(
                    current,
                    Inst::Compare(CmpOp::Eq, label, expected),
                    &[Repr::Bool],
                )
                .pop()
                .expect("a comparison defines one value");

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
        Inst::Const(_) | Inst::Alloc { .. } | Inst::Suspend | Inst::PromiseNew => {}

        Inst::Widen(v) | Inst::Narrow(v, _) => *v = take(),

        Inst::IntArith(_, a, b)
        | Inst::FloatArith(_, a, b)
        | Inst::Bitwise(_, a, b)
        | Inst::Compare(_, a, b)
        | Inst::Generic(_, a, b) => {
            *a = take();
            *b = take();
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
