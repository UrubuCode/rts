//! Building a function.
//!
//! The builder is where the layer's invariants become structural rather than
//! advisory. It refuses at construction what the verifier would reject later,
//! so a mistake is reported where it was made instead of after lowering, with
//! no way to attribute it back.
//!
//! Three refusals carry most of the weight:
//!
//! - arithmetic on a generic operand, because the generic operations exist;
//! - narrowing without a guard, because narrowing can fail;
//! - a branch argument that disagrees with its target, because that is where
//!   representation merges are decided.
//!
//! Widening at a branch is inserted here and only here, which makes it one place
//! to audit rather than one per site.

use super::consts::ConstDecl;
use super::entity::{BlockId, ConstId, ValueId};
use super::func::Function;
use super::inst::{BitOp, BlockCall, CmpOp, GenericOp, Inst, NumOp, Region, Terminator, TrapCode};
use crate::repr::Repr;
use crate::types::{TypeId, TypeRegistry};
use crate::unwind::{Handler, RegionId, Tag};

/// Why a program could not be built.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BuildError {
    /// A proven operation received a generic operand.
    GenericOperand {
        /// The operation that was attempted.
        operation: &'static str,
    },
    /// Operands disagreed about representation.
    MixedOperands {
        /// The operation that was attempted.
        operation: &'static str,
        /// The left operand's representation.
        left: Repr,
        /// The right operand's representation.
        right: Repr,
    },
    /// An operation received a representation it does not apply to.
    WrongDomain {
        /// The operation that was attempted.
        operation: &'static str,
        /// What it received.
        found: Repr,
    },
    /// A branch argument would have to narrow to reach its target.
    ///
    /// Narrowing can fail, so it is only reachable through a guard.
    ImplicitNarrowing {
        /// The block the argument was passed to.
        target: BlockId,
        /// Which argument.
        position: usize,
        /// What the target declared.
        expected: Repr,
        /// What was passed.
        found: Repr,
    },
    /// A branch passed the wrong number of arguments.
    ArgumentCount {
        /// The block the arguments were passed to.
        target: BlockId,
        /// How many it declares.
        expected: usize,
        /// How many were passed.
        found: usize,
    },
    /// A field index does not exist in the aggregate.
    NoSuchField {
        /// The aggregate.
        ty: TypeId,
        /// The index that was asked for.
        field: u32,
    },
    /// A guard's success block does not receive the narrowed value.
    GuardTargetMissingValue {
        /// The block that was named as the success path.
        target: BlockId,
    },
}

/// Result of a building operation.
pub type BuildResult<T> = Result<T, BuildError>;

/// Appends instructions to one block of one function.
pub struct FuncBuilder<'a> {
    func: &'a mut Function,
    types: &'a TypeRegistry,
    block: BlockId,
}

impl<'a> FuncBuilder<'a> {
    /// Starts building at a block.
    pub fn new(func: &'a mut Function, types: &'a TypeRegistry, block: BlockId) -> Self {
        Self { func, types, block }
    }

    /// Moves to another block.
    pub fn switch_to(&mut self, block: BlockId) {
        self.block = block;
    }

    /// The block being appended to.
    pub fn current_block(&self) -> BlockId {
        self.block
    }

    /// Appends an empty block.
    pub fn create_block(&mut self) -> BlockId {
        self.func.push_block()
    }

    /// Appends a parameter to a block.
    pub fn add_block_param(&mut self, block: BlockId, repr: Repr) -> ValueId {
        self.func.push_block_param(block, repr)
    }

    /// Declares a constant.
    pub fn declare_const(&mut self, decl: ConstDecl) -> ConstId {
        self.func.push_const(decl)
    }

    /// Materializes a declared constant.
    pub fn use_const(&mut self, id: ConstId) -> ValueId {
        let repr = self.func.constant(id).expect("constant belongs to this function").repr();
        self.emit(Inst::Const(id), Some(repr))
    }

    /// Arithmetic over two proven operands of identical representation.
    ///
    /// Integer and floating-point domains are distinguished by the operands, not
    /// by the caller naming a domain — the representation already says which.
    pub fn arith(&mut self, op: NumOp, a: ValueId, b: ValueId) -> BuildResult<ValueId> {
        let repr = self.same_proven("arith", a, b)?;
        if repr.is_integer() {
            Ok(self.emit(Inst::IntArith(op, a, b), Some(repr)))
        } else if repr.is_float() {
            Ok(self.emit(Inst::FloatArith(op, a, b), Some(repr)))
        } else {
            Err(BuildError::WrongDomain { operation: "arith", found: repr })
        }
    }

    /// Bitwise operation over two proven integers of identical representation.
    pub fn bitwise(&mut self, op: BitOp, a: ValueId, b: ValueId) -> BuildResult<ValueId> {
        let repr = self.same_proven("bitwise", a, b)?;
        if !repr.is_integer() {
            return Err(BuildError::WrongDomain { operation: "bitwise", found: repr });
        }
        Ok(self.emit(Inst::Bitwise(op, a, b), Some(repr)))
    }

    /// Comparison of two proven operands of identical representation.
    pub fn compare(&mut self, op: CmpOp, a: ValueId, b: ValueId) -> BuildResult<ValueId> {
        self.same_proven("compare", a, b)?;
        Ok(self.emit(Inst::Compare(op, a, b), Some(Repr::Bool)))
    }

    /// An operation on operands whose representation is not proven.
    ///
    /// The only entry point that accepts generic operands. Proven operands are
    /// accepted too and widened, so a client that has proven one side and not
    /// the other does not have to arrange the widening itself.
    pub fn generic(&mut self, op: GenericOp, a: ValueId, b: ValueId) -> ValueId {
        let a = self.widen_if_needed(a);
        let b = self.widen_if_needed(b);
        self.emit(Inst::Generic(op, a, b), Some(Repr::Tagged))
    }

    /// Widens a value into the generic form, or returns it unchanged.
    pub fn widen(&mut self, value: ValueId) -> ValueId {
        self.widen_if_needed(value)
    }

    /// Reads a field of a registered aggregate.
    ///
    /// The result's representation comes from the layout: there is no place to
    /// pass a width, so there is no place to pass the wrong one.
    pub fn field_load(&mut self, object: ValueId, ty: TypeId, field: u32) -> BuildResult<ValueId> {
        let repr = self.field_repr(ty, field)?;
        Ok(self.emit(Inst::FieldLoad { object, ty, field }, Some(repr)))
    }

    /// Writes a field of a registered aggregate.
    ///
    /// A value that does not already match the field is widened; a value that
    /// would have to narrow is refused, for the same reason narrowing is refused
    /// at a branch.
    pub fn field_store(
        &mut self,
        object: ValueId,
        ty: TypeId,
        field: u32,
        value: ValueId,
    ) -> BuildResult<()> {
        let expected = self.field_repr(ty, field)?;
        let found = self.func.repr_of(value);
        let value = match (expected, found) {
            (e, f) if e == f => value,
            (Repr::Tagged, _) => self.widen_if_needed(value),
            (e, f) => {
                return Err(BuildError::WrongDomain {
                    operation: "field_store",
                    found: e.join(f),
                });
            }
        };
        self.emit_effect(Inst::FieldStore { object, ty, field, value });
        Ok(())
    }

    /// Allocates an instance of a registered aggregate.
    pub fn alloc(&mut self, ty: TypeId, region: Region) -> ValueId {
        let repr = Repr::Ref(crate::repr::RefKind::Aggregate(ty));
        self.emit(Inst::Alloc { ty, region }, Some(repr))
    }

    /// Transfers control unconditionally.
    pub fn jump(&mut self, target: BlockId, args: &[ValueId]) -> BuildResult<()> {
        let call = self.block_call(target, args)?;
        self.func.set_terminator(self.block, Terminator::Jump(call));
        Ok(())
    }

    /// Transfers control on a proven boolean.
    pub fn branch(
        &mut self,
        cond: ValueId,
        then_block: (BlockId, &[ValueId]),
        else_block: (BlockId, &[ValueId]),
    ) -> BuildResult<()> {
        let cond_repr = self.func.repr_of(cond);
        if cond_repr != Repr::Bool {
            return Err(BuildError::WrongDomain { operation: "branch", found: cond_repr });
        }
        let then_call = self.block_call(then_block.0, then_block.1)?;
        let else_call = self.block_call(else_block.0, else_block.1)?;
        self.func.set_terminator(
            self.block,
            Terminator::Branch { cond, then_block: then_call, else_block: else_call },
        );
        Ok(())
    }

    /// Tests a generic value's representation, narrowing it on the success path.
    ///
    /// The success block must declare the expected representation as its first
    /// parameter: that is how the narrowed value comes to exist only where the
    /// test held. The failure path is a required argument, which is what makes
    /// a guard a decision rather than an assumption.
    pub fn guard(
        &mut self,
        input: ValueId,
        expect: Repr,
        ok: (BlockId, &[ValueId]),
        fail: (BlockId, &[ValueId]),
    ) -> BuildResult<()> {
        let input = self.widen_if_needed(input);

        let ok_params = self.block_param_reprs(ok.0);
        if ok_params.first() != Some(&expect) {
            return Err(BuildError::GuardTargetMissingValue { target: ok.0 });
        }

        // The narrowed value is supplied by the guard itself, so the caller's
        // arguments fill the block's remaining parameters.
        let ok_call = self.block_call_from(ok.0, ok.1, 1)?;
        let fail_call = self.block_call(fail.0, fail.1)?;
        self.func.set_terminator(
            self.block,
            Terminator::Guard { input, expect, ok: ok_call, fail: fail_call },
        );
        Ok(())
    }

    /// Parks the frame, yielding the value resumption delivers.
    ///
    /// What the frame preserves while parked is not passed here: it is derived
    /// from what is live across this point, for the same reason a root set is
    /// derived rather than declared. A client listing its own live values would
    /// eventually list them wrong, and the failure would be a value quietly
    /// missing after a resumption.
    pub fn suspend(&mut self) -> ValueId {
        self.emit(Inst::Suspend, Some(Repr::Tagged))
    }

    /// Declares a protected region.
    pub fn declare_region(
        &mut self,
        parent: Option<RegionId>,
        handlers: Vec<Handler>,
        cleanup: Option<BlockId>,
    ) -> RegionId {
        self.func.regions.declare(parent, handlers, cleanup)
    }

    /// Places a block inside a protected region.
    pub fn place_in_region(&mut self, block: BlockId, region: RegionId) {
        self.func.set_block_region(block, region);
    }

    /// Throws a value.
    ///
    /// The value is widened if it is not already generic: this layer does not
    /// know what may be thrown, so what travels is the uniform form. Where it
    /// lands is not decided here — it follows from the region the throwing block
    /// is in, which is why there is no destination to pass.
    pub fn throw(&mut self, tag: Tag, payload: ValueId) {
        let payload = self.widen_if_needed(payload);
        self.func.set_terminator(self.block, Terminator::Throw { tag, payload });
    }

    /// Returns from the function.
    pub fn ret(&mut self, values: &[ValueId]) {
        self.func.set_terminator(self.block, Terminator::Return(values.to_vec()));
    }

    /// Stops the program.
    pub fn trap(&mut self, code: TrapCode) {
        self.func.set_terminator(self.block, Terminator::Trap(code));
    }

    fn emit(&mut self, inst: Inst, result: Option<Repr>) -> ValueId {
        self.func
            .push_inst(self.block, inst, result)
            .expect("an instruction declaring a result binds one")
    }

    fn emit_effect(&mut self, inst: Inst) {
        self.func.push_inst(self.block, inst, None);
    }

    fn widen_if_needed(&mut self, value: ValueId) -> ValueId {
        if self.func.repr_of(value) == Repr::Tagged {
            value
        } else {
            self.emit(Inst::Widen(value), Some(Repr::Tagged))
        }
    }

    fn same_proven(&self, operation: &'static str, a: ValueId, b: ValueId) -> BuildResult<Repr> {
        let left = self.func.repr_of(a);
        let right = self.func.repr_of(b);
        if left == Repr::Tagged || right == Repr::Tagged {
            return Err(BuildError::GenericOperand { operation });
        }
        if left != right {
            return Err(BuildError::MixedOperands { operation, left, right });
        }
        Ok(left)
    }

    fn field_repr(&self, ty: TypeId, field: u32) -> BuildResult<Repr> {
        self.types
            .layout(ty)
            .field(field as usize)
            .map(|f| f.repr)
            .ok_or(BuildError::NoSuchField { ty, field })
    }

    fn block_param_reprs(&self, block: BlockId) -> Vec<Repr> {
        self.func
            .block(block)
            .expect("block belongs to this function")
            .params
            .iter()
            .map(|&p| self.func.repr_of(p))
            .collect()
    }

    fn block_call(&mut self, target: BlockId, args: &[ValueId]) -> BuildResult<BlockCall> {
        self.block_call_from(target, args, 0)
    }

    /// Matches `args` against the target's parameters starting at `skip`,
    /// widening where the target is generic and refusing where it would narrow.
    fn block_call_from(
        &mut self,
        target: BlockId,
        args: &[ValueId],
        skip: usize,
    ) -> BuildResult<BlockCall> {
        let expected = self.block_param_reprs(target);
        let expected = &expected[skip.min(expected.len())..];
        if expected.len() != args.len() {
            return Err(BuildError::ArgumentCount {
                target,
                expected: expected.len(),
                found: args.len(),
            });
        }

        let mut converted = Vec::with_capacity(args.len());
        for (position, (&arg, &want)) in args.iter().zip(expected).enumerate() {
            let have = self.func.repr_of(arg);
            converted.push(match (want, have) {
                (w, h) if w == h => arg,
                (Repr::Tagged, _) => self.widen_if_needed(arg),
                (w, h) => {
                    return Err(BuildError::ImplicitNarrowing {
                        target,
                        position: position + skip,
                        expected: w,
                        found: h,
                    });
                }
            });
        }
        Ok(BlockCall { block: target, args: converted })
    }
}
