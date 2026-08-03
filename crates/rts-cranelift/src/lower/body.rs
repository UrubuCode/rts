//! Lowering a function body.
//!
//! Blocks are created before anything is filled in, because a branch can name a
//! block that has not been built yet and a two-pass walk is simpler to be sure
//! of than an ordering rule.

use std::collections::HashMap;

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{Block, InstBuilder, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::error::{Capability, LowerError};
use super::memory;
use super::types::machine_type;
use super::value;
use crate::ir::{
    BitOp, BlockId, CmpOp, ConstDecl, Function, Inst, NumOp, ScalarBits, Terminator, TrapCode,
    ValueId,
};
use crate::repr::Repr;
use crate::symbols::RtEntry;
use crate::target::{Declarations, FunctionRefs};
use cranelift_module::Module;

/// The module a function may name things in, and what it has named.
pub struct Outside<'a> {
    /// Where to ask for a reference to something declared.
    pub module: &'a mut dyn Module,
    /// Which of our identifiers the module has seen.
    pub declarations: &'a Declarations,
    /// Which runtime entry points the module has been told about.
    ///
    /// Mutable because they are declared on first use: a program that never
    /// allocates should leave no reference to an allocator, which matters on the
    /// path where every undefined symbol is something a linker must find.
    pub entries: &'a mut crate::symbols::EntryTable,
}

/// Translates one function's body into the code generator's blocks.
pub struct Body<'a> {
    func: &'a Function,
    blocks: HashMap<BlockId, Block>,
    values: HashMap<ValueId, Value>,
    /// What this function may name outside itself, and where to ask for it.
    ///
    /// Absent when lowering a function on its own. The two travel together
    /// because neither is any use without the other: knowing what has been
    /// declared is pointless without the module that declared it, and the module
    /// alone cannot say which of our identifiers means what.
    outside: Option<Outside<'a>>,
    /// The heap this function may read and write.
    heap: Option<super::Heap<'a>>,
    refs: FunctionRefs,
}

impl<'a> Body<'a> {
    /// Lowers a function in the environment it is allowed to reach.
    pub fn lower_with(
        func: &'a Function,
        builder: &mut FunctionBuilder,
        environment: super::Environment<'a>,
    ) -> Result<(), LowerError> {
        let mut body = Body {
            func,
            blocks: HashMap::new(),
            values: HashMap::new(),
            outside: environment.outside,
            heap: environment.heap,
            refs: FunctionRefs::new(),
        };
        body.create_blocks(builder);
        body.bind_entry_params(builder);

        for (id, _) in func.blocks() {
            body.lower_block(builder, id)?;
        }
        builder.seal_all_blocks();
        Ok(())
    }

    /// Creates every block with its parameters, before any is filled.
    fn create_blocks(&mut self, builder: &mut FunctionBuilder) {
        for (id, data) in self.func.blocks() {
            let block = builder.create_block();
            for &param in &data.params {
                let value = builder.append_block_param(block, machine_type(self.repr(param)));
                self.values.insert(param, value);
            }
            self.blocks.insert(id, block);
        }
    }

    /// Binds the entry block's parameters to the function's own.
    ///
    /// The entry block's parameters *are* the function's parameters — there is
    /// no second way to name them — so this is where the two become the same
    /// values rather than two lists that must agree.
    fn bind_entry_params(&mut self, builder: &mut FunctionBuilder) {
        let entry = self.blocks[&self.func.entry];
        builder.switch_to_block(entry);
    }

    fn lower_block(
        &mut self,
        builder: &mut FunctionBuilder,
        id: BlockId,
    ) -> Result<(), LowerError> {
        let data = self.func.block(id).expect("block belongs to this function");
        builder.switch_to_block(self.blocks[&id]);

        for &inst_id in &data.insts {
            let inst = self
                .func
                .inst(inst_id)
                .expect("instruction belongs to this function");
            self.lower_inst(builder, inst_id, &inst.inst, &inst.results)?;
        }

        match &data.terminator {
            Some(terminator) => self.lower_terminator(builder, id, terminator),
            None => Err(LowerError::UnterminatedBlock { block: id }),
        }
    }

    fn lower_inst(
        &mut self,
        builder: &mut FunctionBuilder,
        id: crate::ir::InstId,
        inst: &Inst,
        results: &[ValueId],
    ) -> Result<(), LowerError> {
        let produced = match inst {
            Inst::Const(decl) => {
                let decl = self.func.constant(*decl).expect("constant belongs here");
                self.lower_const(builder, id, decl)?
            }

            Inst::IntArith(op, a, b) => {
                let (x, y) = (self.value(*a), self.value(*b));
                match op {
                    NumOp::Add => builder.ins().iadd(x, y),
                    NumOp::Sub => builder.ins().isub(x, y),
                    NumOp::Mul => builder.ins().imul(x, y),
                    NumOp::Div => builder.ins().sdiv(x, y),
                }
            }

            Inst::FloatArith(op, a, b) => {
                let (x, y) = (self.value(*a), self.value(*b));
                match op {
                    NumOp::Add => builder.ins().fadd(x, y),
                    NumOp::Sub => builder.ins().fsub(x, y),
                    NumOp::Mul => builder.ins().fmul(x, y),
                    NumOp::Div => builder.ins().fdiv(x, y),
                }
            }

            Inst::Bitwise(op, a, b) => {
                let (x, y) = (self.value(*a), self.value(*b));
                match op {
                    BitOp::And => builder.ins().band(x, y),
                    BitOp::Or => builder.ins().bor(x, y),
                    BitOp::Xor => builder.ins().bxor(x, y),
                    BitOp::Shl => builder.ins().ishl(x, y),
                    BitOp::Shr => builder.ins().sshr(x, y),
                }
            }

            Inst::Compare(op, a, b) => {
                let repr = self.repr(*a);
                let (x, y) = (self.value(*a), self.value(*b));
                let raw = if repr.is_float() {
                    builder.ins().fcmp(float_cc(*op), x, y)
                } else {
                    builder.ins().icmp(int_cc(*op), x, y)
                };
                value::to_machine_bool(builder, raw)
            }

            Inst::Widen(v) => {
                let repr = self.repr(*v);
                let raw = self.value(*v);
                value::widen(builder, raw, repr)?
            }

            Inst::Narrow(v, to) => {
                let raw = self.value(*v);
                value::narrow(builder, raw, *to)?
            }

            Inst::Generic(..) => {
                return Err(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Calls,
                });
            }
            Inst::FieldLoad { object, ty, field } => {
                let heap = self.heap.as_ref().ok_or(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Memory,
                })?;
                let layout = crate::mem::ObjectLayout::of(*ty, heap.types);
                let offset = layout
                    .field_offset(*field)
                    .ok_or(LowerError::NoSuchField { inst: id })?;
                let repr = heap
                    .types
                    .layout(*ty)
                    .field(*field as usize)
                    .map(|f| f.repr);
                let repr = repr.ok_or(LowerError::NoSuchField { inst: id })?;

                let reference = self.value(*object);
                let address = memory::address_of(builder, reference, heap.bases);
                memory::field_load(builder, address, offset, repr)
            }

            Inst::FieldStore {
                object,
                ty,
                field,
                value,
            } => {
                let heap = self.heap.as_ref().ok_or(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Memory,
                })?;
                let layout = crate::mem::ObjectLayout::of(*ty, heap.types);
                let offset = layout
                    .field_offset(*field)
                    .ok_or(LowerError::NoSuchField { inst: id })?;

                let traced = heap
                    .types
                    .layout(*ty)
                    .field(*field as usize)
                    .is_some_and(|f| f.repr.is_gc_relevant());

                let reference = self.value(*object);
                let written = self.value(*value);
                let address = memory::address_of(builder, reference, heap.bases);
                memory::field_store(builder, address, offset, written);

                if traced {
                    self.emit_barrier(builder, id, reference, written)?;
                }
                return Ok(());
            }

            // Allocation is the one memory operation that is not arithmetic: it
            // has to ask a heap for space. So it is the one that is a call.
            Inst::Alloc { ty, .. } => {
                let size = {
                    let heap = self.heap.as_ref().ok_or(LowerError::NotYetLowered {
                        inst: id,
                        needs: Capability::Memory,
                    })?;
                    crate::mem::ObjectLayout::of(*ty, heap.types).size
                };
                let outside = self.outside.as_mut().ok_or(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Memory,
                })?;

                let allocate = outside
                    .entries
                    .reference(outside.module, builder.func, RtEntry::Alloc)
                    .map_err(|_| LowerError::UndeclaredCallee { inst: id })?;
                let size = builder.ins().iconst(types::I64, i64::from(size));
                let ty_id = builder.ins().iconst(types::I64, ty.index() as i64);
                let call = builder.ins().call(allocate, &[size, ty_id]);
                return self.bind_results(builder, call, results);
            }
            Inst::Call { callee, args } => {
                let outside = self.outside.as_mut().ok_or(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Calls,
                })?;
                let reference = self
                    .refs
                    .callee(outside.module, builder.func, outside.declarations, *callee)
                    .ok_or(LowerError::UndeclaredCallee { inst: id })?;
                let args = self.args(args);
                let call = builder.ins().call(reference, &args);
                return self.bind_results(builder, call, results);
            }

            Inst::CallIndirect { callee, sig, args } => {
                let outside = self.outside.as_mut().ok_or(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Calls,
                })?;
                let shape = self
                    .refs
                    .shape(builder.func, outside.declarations, *sig)
                    .ok_or(LowerError::UndeclaredCallee { inst: id })?;
                let target = self.value(*callee);
                let args = self.args(args);
                let call = builder.ins().call_indirect(shape, target, &args);
                return self.bind_results(builder, call, results);
            }
            Inst::PromiseNew => {
                let call = self.call_entry(builder, id, RtEntry::PromiseNew, &[])?;
                return self.bind_results(builder, call, results);
            }

            Inst::PromiseSettle {
                promise,
                value,
                rejected,
            } => {
                let promise = self.value(*promise);
                let value = self.value(*value);
                let rejected = builder.ins().iconst(types::I64, i64::from(*rejected));
                self.call_entry(
                    builder,
                    id,
                    RtEntry::PromiseSettle,
                    &[promise, value, rejected],
                )?;
                return Ok(());
            }

            Inst::Await { promise } => {
                let promise = self.value(*promise);
                let call = self.call_entry(builder, id, RtEntry::PromiseAwait, &[promise])?;
                return self.bind_results(builder, call, results);
            }

            // A bare suspension has no promise, so nothing decides when it
            // resumes. Expressing that needs the frame transformation, which is
            // the one piece of the three that is not a call — see the module doc.
            Inst::Suspend => {
                return Err(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Suspension,
                });
            }
        };

        if let Some(&result) = results.first() {
            self.values.insert(result, produced);
        }
        Ok(())
    }

    fn lower_const(
        &self,
        builder: &mut FunctionBuilder,
        id: crate::ir::InstId,
        decl: &ConstDecl,
    ) -> Result<Value, LowerError> {
        match decl {
            ConstDecl::Scalar {
                repr,
                bits: ScalarBits(bits),
            } => Ok(match repr {
                Repr::F32 => builder.ins().f32const(f32::from_bits(*bits as u32)),
                Repr::F64 => builder.ins().f64const(f64::from_bits(*bits)),
                other => builder.ins().iconst(machine_type(*other), *bits as i64),
            }),
            // Everything else lives in a data section, which needs a module.
            _ => Err(LowerError::NotYetLowered {
                inst: id,
                needs: Capability::Memory,
            }),
        }
    }

    fn lower_terminator(
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

            Terminator::Return(values) => {
                let values = self.args(values);
                builder.ins().return_(&values);
            }

            Terminator::Trap(code) => {
                builder.ins().trap(trap_code(*code));
            }

            Terminator::TailCall { callee, args } => {
                let outside = self
                    .outside
                    .as_mut()
                    .ok_or(LowerError::TerminatorNotYetLowered {
                        block,
                        needs: Capability::Calls,
                    })?;
                let reference = self
                    .refs
                    .callee(outside.module, builder.func, outside.declarations, *callee)
                    .ok_or(LowerError::UndeclaredTailCallee { block })?;
                let args = self.args(args);
                builder.ins().return_call(reference, &args);
            }

            Terminator::TailCallIndirect { callee, sig, args } => {
                let outside = self
                    .outside
                    .as_mut()
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

                // A cleanup block ends with its own terminator, so it can be
                // entered from one place and cannot return to several throw
                // sites. Emitting a chain of them therefore needs either a copy
                // per site or a parameter saying where to continue — and which
                // of those is right is a decision about the representation, not
                // about lowering. Refused here rather than guessed at.
                if !plan.cleanups.is_empty() {
                    return Err(LowerError::CleanupNeedsAContinuation { block });
                }

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
                    // problem — and finding out whose is what the runtime is for.
                    None => {
                        let tag = builder.ins().iconst(types::I64, i64::from(tag.0));
                        self.terminator_entry(builder, block, RtEntry::Throw, &[tag, payload])?;
                        builder
                            .ins()
                            .trap(cranelift_codegen::ir::TrapCode::unwrap_user(1));
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
            .as_mut()
            .ok_or(LowerError::TerminatorNotYetLowered {
                block,
                needs: Capability::Unwinding,
            })?;
        let reference = outside
            .entries
            .reference(outside.module, builder.func, entry)
            .map_err(|_| LowerError::UndeclaredTailCallee { block })?;
        builder.ins().call(reference, args);
        Ok(())
    }

    /// Calls a runtime entry point.
    ///
    /// One place, so that declaring an entry point on first use and referring to
    /// it afterwards is a fact about the layer rather than a discipline each
    /// emission site follows.
    fn call_entry(
        &mut self,
        builder: &mut FunctionBuilder,
        id: crate::ir::InstId,
        entry: RtEntry,
        args: &[Value],
    ) -> Result<cranelift_codegen::ir::Inst, LowerError> {
        let outside = self.outside.as_mut().ok_or(LowerError::NotYetLowered {
            inst: id,
            needs: Capability::Calls,
        })?;
        let reference = outside
            .entries
            .reference(outside.module, builder.func, entry)
            .map_err(|_| LowerError::UndeclaredCallee { inst: id })?;
        Ok(builder.ins().call(reference, args))
    }

    /// Emits the barrier a reference store owes the collector.
    ///
    /// Unconditionally, for every store into a traced field. That is more than
    /// strictly necessary — a store into an object no other thread can reach
    /// makes nothing visible anywhere new, and could skip it — but deciding that
    /// needs to know the object's region, and a reference does not carry one.
    /// The ownership annotation that would answer it does not exist yet.
    ///
    /// So the direction of the error is chosen deliberately. A barrier that was
    /// not needed costs a call. A barrier that was needed and skipped produces a
    /// reference the collector never learns about, which becomes a use-after-free
    /// that reproduces rarely and explains nothing. The first is a bill; the
    /// second is a bug nobody can find.
    fn emit_barrier(
        &mut self,
        builder: &mut FunctionBuilder,
        id: crate::ir::InstId,
        object: Value,
        written: Value,
    ) -> Result<(), LowerError> {
        let outside = self.outside.as_mut().ok_or(LowerError::NotYetLowered {
            inst: id,
            needs: Capability::Memory,
        })?;
        let barrier = outside
            .entries
            .reference(outside.module, builder.func, RtEntry::WriteBarrier)
            .map_err(|_| LowerError::UndeclaredCallee { inst: id })?;
        builder.ins().call(barrier, &[object, written]);
        Ok(())
    }

    /// Binds a call's results to the values the instruction defines.
    fn bind_results(
        &mut self,
        builder: &FunctionBuilder,
        call: cranelift_codegen::ir::Inst,
        results: &[ValueId],
    ) -> Result<(), LowerError> {
        let produced = builder.inst_results(call);
        for (&ours, &theirs) in results.iter().zip(produced) {
            self.values.insert(ours, theirs);
        }
        Ok(())
    }

    fn value(&self, id: ValueId) -> Value {
        self.values[&id]
    }

    fn args(&self, ids: &[ValueId]) -> Vec<Value> {
        ids.iter().map(|&id| self.value(id)).collect()
    }

    /// Branch arguments, in the form a branch takes them.
    ///
    /// The code generator distinguishes a value passed along an edge from a
    /// value used in place, because an edge can also carry things a value cannot
    /// be. Everything this layer passes is an ordinary value, so the conversion
    /// is total — but it is written once here rather than at each branch.
    fn block_args(&self, ids: &[ValueId]) -> Vec<cranelift_codegen::ir::BlockArg> {
        ids.iter()
            .map(|&id| cranelift_codegen::ir::BlockArg::Value(self.value(id)))
            .collect()
    }

    fn repr(&self, id: ValueId) -> Repr {
        self.func.repr_of(id)
    }
}

/// The integer condition a comparison uses.
///
/// Signed throughout: the representations are signed integers, and choosing an
/// unsigned condition for a signed value is the kind of silent wrongness this
/// layer refuses elsewhere.
fn int_cc(op: CmpOp) -> IntCC {
    match op {
        CmpOp::Eq => IntCC::Equal,
        CmpOp::Ne => IntCC::NotEqual,
        CmpOp::Lt => IntCC::SignedLessThan,
        CmpOp::Le => IntCC::SignedLessThanOrEqual,
        CmpOp::Gt => IntCC::SignedGreaterThan,
        CmpOp::Ge => IntCC::SignedGreaterThanOrEqual,
    }
}

/// The floating-point condition a comparison uses.
///
/// Ordered, so that a comparison against a NaN is false rather than true — which
/// is what "less than" means when one side is not a number.
fn float_cc(op: CmpOp) -> FloatCC {
    match op {
        CmpOp::Eq => FloatCC::Equal,
        CmpOp::Ne => FloatCC::NotEqual,
        CmpOp::Lt => FloatCC::LessThan,
        CmpOp::Le => FloatCC::LessThanOrEqual,
        CmpOp::Gt => FloatCC::GreaterThan,
        CmpOp::Ge => FloatCC::GreaterThanOrEqual,
    }
}

/// The code generator's name for why a program stopped.
fn trap_code(code: TrapCode) -> cranelift_codegen::ir::TrapCode {
    use cranelift_codegen::ir::TrapCode as Cl;
    match code {
        TrapCode::Unreachable => Cl::unwrap_user(1),
        TrapCode::OutOfBounds => Cl::HEAP_OUT_OF_BOUNDS,
        TrapCode::DivideByZero => Cl::INTEGER_DIVISION_BY_ZERO,
    }
}
