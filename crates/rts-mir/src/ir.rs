//! MIR IR types — `MirFunc`, `BasicBlock`, `Inst`, `Terminator`.
//!
//! Each `Inst` variant maps 1:1 to a family of Cranelift instructions so the
//! later lowering pass in `rts-codegen` is mechanical. See
//! `RTS_REFACTOR.md` §rts-mir for the exhaustive variant list.

use rts_hir::HirType;

pub type ValueId = u32;
pub type BlockId = u32;

/// Top-level MIR function — list of basic blocks plus type table indexed by
/// `ValueId`.
#[derive(Debug, Clone)]
pub struct MirFunc {
    pub name: String,
    pub conv: CallConvHint,
    pub params: Vec<(ValueId, HirType)>,
    pub ret: HirType,
    pub blocks: Vec<BasicBlock>,
    pub values: Vec<HirType>,
}

impl MirFunc {
    pub fn new(name: impl Into<String>, conv: CallConvHint, ret: HirType) -> Self {
        Self {
            name: name.into(),
            conv,
            params: Vec::new(),
            ret,
            blocks: Vec::new(),
            values: Vec::new(),
        }
    }

    /// Append a fresh value of the given type and return its id.
    pub fn new_value(&mut self, ty: HirType) -> ValueId {
        let id = self.values.len() as ValueId;
        self.values.push(ty);
        id
    }

    /// Append a fresh block (without params) and return its id.
    pub fn new_block(&mut self) -> BlockId {
        let id = self.blocks.len() as BlockId;
        self.blocks.push(BasicBlock {
            id,
            params: Vec::new(),
            insts: Vec::new(),
            term: Terminator::Trap {
                code: TrapHint::User(0),
            },
        });
        id
    }

    pub fn type_of(&self, v: ValueId) -> &HirType {
        &self.values[v as usize]
    }
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub params: Vec<(ValueId, HirType)>,
    pub insts: Vec<Inst>,
    pub term: Terminator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallConvHint {
    Tail,
    SystemV,
    WindowsFastcall,
    Cold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemHint {
    Trusted,
    Notrap,
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemOrder {
    Relaxed,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RmwOp {
    Add, Sub, And, Or, Xor, Nand, Xchg,
    Umin, Umax, Smin, Smax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapHint {
    DivByZero,
    IntOverflow,
    HeapOob,
    StackOverflow,
    User(u16),
}

/// Mirrors `cranelift_codegen::ir::condcodes::IntCC`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntCond {
    Eq, Ne,
    Slt, Sle, Sgt, Sge,
    Ult, Ule, Ugt, Uge,
}

/// Mirrors `cranelift_codegen::ir::condcodes::FloatCC`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatCond {
    Eq, Ne,
    OLt, OLe, OGt, OGe, ONe,
    ULt, ULe, UGt, UGe, UNe,
    Ordered, Unordered,
}

/// MIR instruction. Each variant maps 1:1 to a Cranelift instruction family.
#[derive(Debug, Clone)]
pub enum Inst {
    // Integer arithmetic
    IAdd { dst: ValueId, lhs: ValueId, rhs: ValueId },
    IAddImm { dst: ValueId, lhs: ValueId, imm: i64 },
    ISub { dst: ValueId, lhs: ValueId, rhs: ValueId },
    IMul { dst: ValueId, lhs: ValueId, rhs: ValueId },
    IMulImm { dst: ValueId, lhs: ValueId, imm: i64 },
    SDiv { dst: ValueId, lhs: ValueId, rhs: ValueId },
    UDiv { dst: ValueId, lhs: ValueId, rhs: ValueId },
    SRem { dst: ValueId, lhs: ValueId, rhs: ValueId },
    URem { dst: ValueId, lhs: ValueId, rhs: ValueId },
    INeg { dst: ValueId, src: ValueId },

    // Bitwise
    BAnd { dst: ValueId, lhs: ValueId, rhs: ValueId },
    BAndImm { dst: ValueId, lhs: ValueId, imm: i64 },
    BOr { dst: ValueId, lhs: ValueId, rhs: ValueId },
    BOrImm { dst: ValueId, lhs: ValueId, imm: i64 },
    BXor { dst: ValueId, lhs: ValueId, rhs: ValueId },
    BXorImm { dst: ValueId, lhs: ValueId, imm: i64 },
    BNot { dst: ValueId, src: ValueId },

    // Shifts
    IShl { dst: ValueId, lhs: ValueId, rhs: ValueId },
    IShlImm { dst: ValueId, lhs: ValueId, imm: i64 },
    UShr { dst: ValueId, lhs: ValueId, rhs: ValueId },
    SShr { dst: ValueId, lhs: ValueId, rhs: ValueId },
    SShrImm { dst: ValueId, lhs: ValueId, imm: i64 },
    Rotl { dst: ValueId, lhs: ValueId, rhs: ValueId },
    Rotr { dst: ValueId, lhs: ValueId, rhs: ValueId },

    // Bit counting
    Clz { dst: ValueId, src: ValueId },
    Ctz { dst: ValueId, src: ValueId },
    Popcnt { dst: ValueId, src: ValueId },
    Bswap { dst: ValueId, src: ValueId },

    // Float arithmetic
    FAdd { dst: ValueId, lhs: ValueId, rhs: ValueId },
    FSub { dst: ValueId, lhs: ValueId, rhs: ValueId },
    FMul { dst: ValueId, lhs: ValueId, rhs: ValueId },
    FDiv { dst: ValueId, lhs: ValueId, rhs: ValueId },
    FNeg { dst: ValueId, src: ValueId },
    FAbs { dst: ValueId, src: ValueId },
    Sqrt { dst: ValueId, src: ValueId },
    Fma { dst: ValueId, a: ValueId, b: ValueId, c: ValueId },
    FMin { dst: ValueId, lhs: ValueId, rhs: ValueId },
    FMax { dst: ValueId, lhs: ValueId, rhs: ValueId },
    Ceil { dst: ValueId, src: ValueId },
    Floor { dst: ValueId, src: ValueId },
    Trunc { dst: ValueId, src: ValueId },

    // Conversions
    IReduce { dst: ValueId, src: ValueId, to: HirType },
    UExtend { dst: ValueId, src: ValueId, to: HirType },
    SExtend { dst: ValueId, src: ValueId, to: HirType },
    FPromote { dst: ValueId, src: ValueId },
    FDemote { dst: ValueId, src: ValueId },
    CvtFromSint { dst: ValueId, src: ValueId, to: HirType },
    CvtFromUint { dst: ValueId, src: ValueId, to: HirType },
    CvtToSintSat { dst: ValueId, src: ValueId, to: HirType },
    CvtToUintSat { dst: ValueId, src: ValueId, to: HirType },
    Bitcast { dst: ValueId, src: ValueId, to: HirType },

    // Comparisons
    ICmp { dst: ValueId, cond: IntCond, lhs: ValueId, rhs: ValueId },
    FCmp { dst: ValueId, cond: FloatCond, lhs: ValueId, rhs: ValueId },

    // Constants
    IConst { dst: ValueId, ty: HirType, val: i64 },
    F32Const { dst: ValueId, val: f32 },
    F64Const { dst: ValueId, val: f64 },

    // Memory
    Load { dst: ValueId, ty: HirType, ptr: ValueId, offset: i32, flags: MemHint },
    Store { val: ValueId, ptr: ValueId, offset: i32, flags: MemHint },

    // Narrow loads
    ULoad8 { dst: ValueId, ptr: ValueId, offset: i32 },
    ULoad16 { dst: ValueId, ptr: ValueId, offset: i32 },
    ULoad32 { dst: ValueId, ptr: ValueId, offset: i32 },
    SLoad8 { dst: ValueId, ptr: ValueId, offset: i32 },
    SLoad16 { dst: ValueId, ptr: ValueId, offset: i32 },
    SLoad32 { dst: ValueId, ptr: ValueId, offset: i32 },

    // Narrow stores
    IStore8 { val: ValueId, ptr: ValueId, offset: i32 },
    IStore16 { val: ValueId, ptr: ValueId, offset: i32 },
    IStore32 { val: ValueId, ptr: ValueId, offset: i32 },

    // Stack
    StackAlloc { dst: ValueId, size: u32, align: u8 },

    // Branchless select
    Select { dst: ValueId, cond: ValueId, on_true: ValueId, on_false: ValueId },

    // Extern call (runtime ABI)
    /// Call to an externally-declared function (e.g. an RTS runtime symbol
    /// like `__RTS_FN_NS_MATH_SQRT`). The codegen declares the import via
    /// `Module::declare_function` with `Linkage::Import` using the embedded
    /// `param_tys`/`ret_ty`. `dst = None` for void returns.
    CallExtern {
        dst: Option<ValueId>,
        sym: String,
        args: Vec<ValueId>,
        ret_ty: HirType,
        param_tys: Vec<HirType>,
    },

    /// Materialize a static UTF-8 string literal as a `(ptr, len)` pair.
    /// Used for passing string arguments to extern calls with `StrPtr`
    /// signatures (e.g. `io.print("hello")` → ptr/len → 2 separate args).
    /// The codegen allocates a read-only data segment and emits the
    /// pointer + length into the two destination ValueIds.
    StrLit {
        dst_ptr: ValueId,
        dst_len: ValueId,
        value: String,
    },

    // Atomics
    AtomicLoad { dst: ValueId, ptr: ValueId, order: MemOrder },
    AtomicStore { val: ValueId, ptr: ValueId, order: MemOrder },
    AtomicRmw { dst: ValueId, op: RmwOp, ptr: ValueId, val: ValueId, order: MemOrder },
    AtomicCas { dst: ValueId, ptr: ValueId, expected: ValueId, replacement: ValueId, order: MemOrder },
    Fence { order: MemOrder },

    // GC
    DeclareGcValue { val: ValueId },

    /// Call to a user-defined function declared elsewhere in the same MIR
    /// program. The codegen lowers this to a Cranelift `call` instruction
    /// using a `FuncRef` cached per (caller, target_name) pair.
    ///
    /// `dst = None` when the callee returns Void.
    /// `param_tys` carries the callee signature so codegen doesn't have to
    /// resolve the target separately — duplicates info but keeps each Inst
    /// self-contained.
    CallUser {
        dst: Option<ValueId>,
        name: String,
        args: Vec<ValueId>,
        ret_ty: HirType,
        param_tys: Vec<HirType>,
    },
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Return(Vec<ValueId>),
    Jump {
        target: BlockId,
        args: Vec<ValueId>,
    },
    Brif {
        cond: ValueId,
        then_block: BlockId,
        then_args: Vec<ValueId>,
        else_block: BlockId,
        else_args: Vec<ValueId>,
    },
    Switch {
        index: ValueId,
        default: BlockId,
        cases: Vec<(u64, BlockId)>,
    },
    TailCall {
        sym: String,
        args: Vec<ValueId>,
    },
    TailCallIndirect {
        fn_ptr: ValueId,
        args: Vec<ValueId>,
    },
    Trap {
        code: TrapHint,
    },
}
