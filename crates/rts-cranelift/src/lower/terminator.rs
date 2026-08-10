//! Where a block goes when it ends.
//!
//! Its own module because a terminator is the only place three concerns meet:
//! control flow, the cleanup a scope owes on the way out, and the inline caches
//! whose hit and miss are *branches* rather than instructions. Keeping them
//! beside the arithmetic made one file that was about everything.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, types};
use cranelift_frontend::FunctionBuilder;

use super::body::Body;
use super::body::trap_code;
use super::error::{Capability, LowerError};
use super::{memory, value};
use crate::ir::{BlockId, Terminator};
use crate::repr::Repr;
use crate::symbols::RtEntry;
use cranelift_codegen::ir::Value;

impl Body<'_> {
    pub(super) fn lower_terminator(
        &mut self,
        builder: &mut FunctionBuilder,
        block: BlockId,
        terminator: &Terminator,
    ) -> Result<(), LowerError> {
        match terminator {
            Terminator::Jump(call) => {
                let args = self.block_args(&call.args);
                let target = self.blocks[&call.block];
                builder.ins().jump(target, &args);
            }

            Terminator::Branch {
                cond,
                then_block,
                else_block,
            } => {
                let cond = self.value(*cond);
                let then_args = self.block_args(&then_block.args);
                let else_args = self.block_args(&else_block.args);
                let (t, e) = (
                    self.blocks[&then_block.block],
                    self.blocks[&else_block.block],
                );
                builder.ins().brif(cond, t, &then_args, e, &else_args);
            }

            Terminator::Guard {
                input,
                expect,
                ok,
                fail,
            } => {
                let input = self.value(*input);
                let held = value::test(builder, input, *expect)?;

                // The narrowed value is computed unconditionally and passed on
                // the success edge only. It is pure bit manipulation, so
                // computing it where the test failed is harmless — and cheaper
                // than a block that exists only to compute it.
                let narrowed = value::narrow(builder, input, *expect)?;

                let mut ok_args = vec![cranelift_codegen::ir::BlockArg::Value(narrowed)];
                ok_args.extend(self.block_args(&ok.args));
                let fail_args = self.block_args(&fail.args);
                let (t, e) = (self.blocks[&ok.block], self.blocks[&fail.block]);
                builder.ins().brif(held, t, &ok_args, e, &fail_args);
            }

            Terminator::GuardType {
                object,
                expect,
                ok,
                fail,
            } => {
                let heap = self
                    .heap
                    .as_ref()
                    .ok_or(LowerError::TerminatorNotYetLowered {
                        block,
                        needs: Capability::Memory,
                    })?;
                let reference = self.value(*object);

                // The type is in the object's header, which is where it was put
                // so that a collector could read it without knowing what the
                // object is. The same fact answers this question.
                let address = memory::address_of(builder, reference, heap);
                let header = memory::field_load(
                    builder,
                    address,
                    crate::mem::HeaderLayout::TYPE_OFFSET,
                    Repr::I64,
                );
                let held = builder
                    .ins()
                    .icmp_imm(IntCC::Equal, header, expect.index() as i64);

                // The narrowed value is the same bits: what changed is what is
                // known about them, and nothing known is stored in the value.
                let mut ok_args = vec![cranelift_codegen::ir::BlockArg::Value(reference)];
                ok_args.extend(self.block_args(&ok.args));
                let fail_args = self.block_args(&fail.args);
                let (t, e) = (self.blocks[&ok.block], self.blocks[&fail.block]);
                builder.ins().brif(held, t, &ok_args, e, &fail_args);
            }

            Terminator::CachedGet {
                object,
                key,
                cache,
                hit,
                miss,
            } => {
                self.lower_cached_get(builder, block, *object, *key, *cache, hit, miss)?;
            }

            Terminator::CachedSet {
                object,
                key,
                cache,
                value,
                hit,
                miss,
            } => {
                self.lower_cached_set(builder, block, *object, *key, *cache, *value, hit, miss)?;
            }

            Terminator::Return(values) => {
                // Leaving a region normally owes the same cleanup as leaving it
                // by throwing. A scope that only unwinds correctly when
                // something goes wrong leaks on the path taken most of the time.
                if let Some(region) = self.func.region_of(block) {
                    let chain = crate::unwind::plan_normal_exit(&self.func.regions, region);
                    self.emit_cleanups(builder, &chain)?;
                }

                let values = self.args(values);
                builder.ins().return_(&values);
            }

            Terminator::Trap(code) => {
                builder.ins().trap(trap_code(*code));
            }

            // A cleanup is reached by being copied into the path that needs it,
            // so the original is never entered. Reaching it means something
            // jumped to a block nothing should jump to.
            Terminator::CleanupDone => {
                builder
                    .ins()
                    .trap(cranelift_codegen::ir::TrapCode::unwrap_user(1));
            }

            Terminator::TailCall { callee, args } => {
                let outside = self
                    .outside
                    .as_ref()
                    .ok_or(LowerError::TerminatorNotYetLowered {
                        block,
                        needs: Capability::Calls,
                    })?;
                let reference = self
                    .refs
                    .callee(outside.machine, builder.func, outside.declarations, *callee)
                    .ok_or(LowerError::UndeclaredTailCallee { block })?;
                let args = self.args(args);
                builder.ins().return_call(reference, &args);
            }

            Terminator::TailCallIndirect { callee, sig, args } => {
                let outside = self
                    .outside
                    .as_ref()
                    .ok_or(LowerError::TerminatorNotYetLowered {
                        block,
                        needs: Capability::Calls,
                    })?;
                let shape = self
                    .refs
                    .shape(builder.func, outside.declarations, *sig)
                    .ok_or(LowerError::UndeclaredTailCallee { block })?;
                let target = self.value(*callee);
                let args = self.args(args);
                builder.ins().return_call_indirect(shape, target, &args);
            }

            Terminator::Throw { tag, payload } => {
                let plan = match self.func.region_of(block) {
                    Some(region) => crate::unwind::plan_unwind(&self.func.regions, region, *tag),
                    None => crate::unwind::UnwindPlan {
                        cleanups: Vec::new(),
                        handler: None,
                    },
                };

                // Each cleanup is copied here, in order, innermost first. A copy
                // rather than a jump because a cleanup has no way back: it is
                // entered from every path that unwinds through it, and those
                // paths have nothing in common to return to.
                self.emit_cleanups(builder, &plan.cleanups)?;

                let payload = self.value(*payload);
                match plan.handler {
                    // The destination was computed while compiling, so control
                    // simply goes there. Asking the runtime to search for an
                    // answer already known would be paying for it twice.
                    Some(handler) => {
                        let target = self.blocks[&handler];
                        let args = vec![cranelift_codegen::ir::BlockArg::Value(payload)];
                        builder.ins().jump(target, &args);
                    }

                    // Nothing here catches it, so it becomes the caller's
                    // problem — and this is where it is handed over. The runtime
                    // RECORDS the value, and then this frame simply returns:
                    // a throw leaves one frame at a time, and the frame above
                    // asks whether it did.
                    //
                    // It used to `trap` here, on the reasoning that the entry
                    // point never comes back — it ended the program. That drew
                    // the boundary at one frame and made `try` around a call
                    // uncompilable, because a handler one frame up could never
                    // be reached. Returning is what moved it.
                    //
                    // The returned values are zeros of the declared widths. They
                    // are never read: the caller's check sees a throw in flight
                    // and re-raises before touching what the call produced. A
                    // width that disagreed with the signature would be refused
                    // by the verifier, which is why they are built from it.
                    None => {
                        let tag = builder.ins().iconst(types::I64, i64::from(tag.0));
                        self.terminator_entry(builder, block, RtEntry::Throw, &[tag, payload])?;
                        let results: Vec<_> = self
                            .func
                            .signature
                            .returns
                            .iter()
                            .map(|repr| {
                                let ty = crate::lower::types::machine_type(*repr);
                                match ty {
                                    types::F64 => builder.ins().f64const(0.0),
                                    other => builder.ins().iconst(other, 0),
                                }
                            })
                            .collect();
                        builder.ins().return_(&results);
                    }
                }
            }
        }
        Ok(())
    }

    /// The same, for a terminator, which is named by its block.
    fn terminator_entry(
        &mut self,
        builder: &mut FunctionBuilder,
        block: BlockId,
        entry: RtEntry,
        args: &[Value],
    ) -> Result<(), LowerError> {
        let outside = self
            .outside
            .as_ref()
            .ok_or(LowerError::TerminatorNotYetLowered {
                block,
                needs: Capability::Unwinding,
            })?;
        let reference = self
            .refs
            .entry(outside.machine, outside.entries, builder.func, entry);
        builder.ins().call(reference, args);
        Ok(())
    }
}
