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

    /// Widens a proven value into the generic form.
    ///
    /// Emitted as pure instructions, never a call, so that a redundant pair
    /// around an already-uniform value folds away entirely.
    Widen(ValueId),
    /// Narrows a generic value whose representation a guard has established.
    Narrow(ValueId, Repr),

    /// An operation on operands whose representation is not proven.
    Generic(GenericOp, ValueId, ValueId),

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
}

impl Inst {
    /// The values this instruction reads.
    ///
    /// Every analysis over the IR needs this, and each one deriving it from its
    /// own match over the variants is how an added variant comes to be handled
    /// in three places and forgotten in a fourth.
    pub fn operands(&self) -> Vec<ValueId> {
        match self {
            Inst::Const(_) | Inst::Alloc { .. } | Inst::Suspend => Vec::new(),

            Inst::Widen(v) | Inst::Narrow(v, _) => vec![*v],

            Inst::IntArith(_, a, b)
            | Inst::FloatArith(_, a, b)
            | Inst::Bitwise(_, a, b)
            | Inst::Compare(_, a, b)
            | Inst::Generic(_, a, b) => vec![*a, *b],

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
        matches!(self, Inst::Alloc { .. } | Inst::Suspend)
    }

    /// Whether this instruction parks the frame.
    ///
    /// A suspension is also a safepoint, and for a sharper reason than an
    /// allocation is: a parked frame can sit there across any number of
    /// collections, so what it holds must be findable for as long as it is
    /// parked — not merely at the moment it stops.
    pub fn is_suspend(&self) -> bool {
        matches!(self, Inst::Suspend)
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
        Self { block, args: Vec::new() }
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
            Terminator::Branch { then_block, else_block, .. } => {
                vec![then_block.block, else_block.block]
            }
            Terminator::Guard { ok, fail, .. } => vec![ok.block, fail.block],
            // A throw has no successor in this function's graph. Where it lands
            // is decided by the region tree, and may be in a caller; calling it
            // an edge here would claim a transfer this block does not perform.
            Terminator::Return(_) | Terminator::Throw { .. } | Terminator::Trap(_) => Vec::new(),
        }
    }

    /// The values this terminator reads, including branch arguments.
    pub fn operands(&self) -> Vec<ValueId> {
        let mut operands = Vec::new();
        match self {
            Terminator::Jump(call) => operands.extend_from_slice(&call.args),
            Terminator::Branch { cond, then_block, else_block } => {
                operands.push(*cond);
                operands.extend_from_slice(&then_block.args);
                operands.extend_from_slice(&else_block.args);
            }
            Terminator::Guard { input, ok, fail, .. } => {
                operands.push(*input);
                operands.extend_from_slice(&ok.args);
                operands.extend_from_slice(&fail.args);
            }
            Terminator::Return(values) => operands.extend_from_slice(values),
            Terminator::Throw { payload, .. } => operands.push(*payload),
            Terminator::Trap(_) => {}
        }
        operands
    }
}

/// An instruction together with the value it defines, if any.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InstData {
    /// The operation.
    pub inst: Inst,
    /// The value it defines, absent for instructions that only have an effect.
    pub result: Option<ValueId>,
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
