//! Lowering a function body.
//!
//! Blocks are created before anything is filled in, because a branch can name a
//! block that has not been built yet and a two-pass walk is simpler to be sure
//! of than an ordering rule.

use std::collections::HashMap;

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{Block, InstBuilder, Value};
use cranelift_frontend::FunctionBuilder;

use super::error::{Capability, LowerError};
use super::types::machine_type;
use super::value;
use crate::ir::{
    BitOp, BlockId, CmpOp, ConstDecl, Function, Inst, NumOp, ScalarBits, Terminator, TrapCode,
    ValueId,
};
use crate::repr::Repr;
use crate::target::{Declarations, FunctionRefs};
use cranelift_module::Module;

/// The module a function may name things in, and what it has named.
pub struct Outside<'a> {
    /// Where to ask for a reference to something declared.
    pub module: &'a mut dyn Module,
    /// Which of our identifiers the module has seen.
    pub declarations: &'a Declarations,
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
    refs: FunctionRefs,
}

impl<'a> Body<'a> {
    /// Lowers a function that names nothing outside itself.
    pub fn lower(func: &'a Function, builder: &mut FunctionBuilder) -> Result<(), LowerError> {
        Self::lower_with(func, builder, None)
    }

    /// Lowers a function that may call what a module has declared.
    pub fn lower_with(
        func: &'a Function,
        builder: &mut FunctionBuilder,
        outside: Option<Outside<'a>>,
    ) -> Result<(), LowerError> {
        let mut body = Body {
            func,
            blocks: HashMap::new(),
            values: HashMap::new(),
            outside,
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
            Inst::Alloc { .. } | Inst::FieldLoad { .. } | Inst::FieldStore { .. } => {
                return Err(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Memory,
                });
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
            Inst::Suspend | Inst::Await { .. } => {
                return Err(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Suspension,
                });
            }
            Inst::PromiseNew | Inst::PromiseSettle { .. } => {
                return Err(LowerError::NotYetLowered {
                    inst: id,
                    needs: Capability::Scheduling,
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

            Terminator::Throw { .. } => {
                return Err(LowerError::TerminatorNotYetLowered {
                    block,
                    needs: Capability::Unwinding,
                });
            }
        }
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
