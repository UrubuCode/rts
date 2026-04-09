pub mod build;
pub mod cfg;
pub mod monomorphize;
pub mod optimize;
pub mod typed_build;

#[derive(Debug, Clone, Default)]
pub struct MirModule {
    pub functions: Vec<MirFunction>,
}

#[derive(Debug, Clone, Default)]
pub struct MirFunction {
    pub name: String,
    pub blocks: Vec<cfg::BasicBlock>,
}

#[derive(Debug, Clone, Default)]
pub struct MirStatement {
    pub text: String,
}

// ── Typed MIR data structures (Phase 1) ──────────────────────────────

/// Virtual register identifier for SSA-style MIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VReg(pub u32);

/// Binary operations that can be compiled natively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
    Ne,
    LogicAnd,
    LogicOr,
}

/// Unary operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirUnaryOp {
    Negate,
    Not,
    Positive,
}

/// Type tag carried on typed MIR instructions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirType {
    Number,
    Int,
    Bool,
    String,
    Null,
    Undefined,
    Handle,
    Unknown,
}

/// A single typed MIR instruction.
#[derive(Debug, Clone)]
pub enum MirInstruction {
    ConstNumber(VReg, f64),
    ConstString(VReg, String),
    ConstBool(VReg, bool),
    ConstNull(VReg),
    ConstUndef(VReg),
    LoadParam(VReg, usize),
    BinOp(VReg, MirBinOp, VReg, VReg),
    UnaryOp(VReg, MirUnaryOp, VReg),
    Call(VReg, String, Vec<VReg>),
    Bind(String, VReg, bool),
    LoadBinding(VReg, String),
    /// Write to an existing mutable binding: write "<name>" = %src
    WriteBind(String, VReg),
    Return(Option<VReg>),
    Import { names: Vec<String>, from: String },
    RuntimeEval(VReg, String),
}

/// A basic block using typed instructions.
#[derive(Debug, Clone)]
pub struct TypedBasicBlock {
    pub label: String,
    pub instructions: Vec<MirInstruction>,
    pub terminator: cfg::Terminator,
}

/// A function using typed MIR instructions.
#[derive(Debug, Clone)]
pub struct TypedMirFunction {
    pub name: String,
    pub param_count: usize,
    pub blocks: Vec<TypedBasicBlock>,
    pub next_vreg: u32,
}

impl TypedMirFunction {
    pub fn alloc_vreg(&mut self) -> VReg {
        let reg = VReg(self.next_vreg);
        self.next_vreg += 1;
        reg
    }
}

/// Module-level typed MIR.
#[derive(Debug, Clone, Default)]
pub struct TypedMirModule {
    pub functions: Vec<TypedMirFunction>,
}
