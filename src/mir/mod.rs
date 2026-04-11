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

/// SIMD vector size for operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdWidth {
    V128, // 128-bit vectors (4x f32 or 2x f64 or 4x i32)
    V256, // 256-bit vectors (8x f32 or 4x f64 or 8x i32) - AVX
}

/// SIMD operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdOp {
    Add,
    Sub,
    Mul,
    Div,
    Max,
    Min,
    Sqrt,
    FMA, // Fused multiply-add
}

/// A single typed MIR instruction.
#[derive(Debug, Clone)]
pub enum MirInstruction {
    ConstNumber(VReg, f64),
    ConstInt32(VReg, i32),
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
    Import {
        names: Vec<String>,
        from: String,
    },
    /// Jump to a labeled block unconditionally
    Jump(String),
    /// Conditional jump based on boolean register
    JumpIf(VReg, String),
    /// Jump if register is falsy
    JumpIfNot(VReg, String),
    /// Label marker for jump targets
    Label(String),
    /// Break from current loop context
    Break,
    /// Continue to next iteration of current loop
    Continue,
    /// SIMD vector operations for parallel arithmetic
    SimdConst(VReg, SimdWidth, Vec<f64>), // Load vector constant
    SimdOp(VReg, SimdOp, SimdWidth, VReg, VReg), // SIMD binary operation
    SimdLoad(VReg, SimdWidth, VReg, i32),        // Load vector from memory[base + offset]
    SimdStore(SimdWidth, VReg, VReg, i32),       // Store vector to memory[base + offset]
    /// Loop unrolling hint with factor
    UnrollHint(u32),
    /// Mark the beginning of a hot loop for optimization
    LoopBegin(String), // loop_id
    /// Mark the end of a hot loop
    LoopEnd(String), // loop_id
    /// Strength reduction hint - replace expensive operations with cheaper ones
    StrengthReduce(VReg, MirBinOp, VReg, VReg), // expensive_op -> cheaper_alternative
    /// Hoist loop-invariant computation out of loop
    HoistInvariant(VReg, String), // vreg, loop_id
    /// Mark function as inline candidate
    InlineCandidate(String), // function_name
    /// Inline function call directly instead of calling
    InlineCall(VReg, String, Vec<VReg>), // dst, inlined_function_name, args
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
    /// Arquivo TypeScript de origem (propagado do HIR via SourceLocation).
    pub source_file: Option<String>,
    /// Linha de declaração da função no arquivo fonte.
    pub source_line: u32,
}

impl TypedMirFunction {
    pub fn alloc_vreg(&mut self) -> VReg {
        let reg = VReg(self.next_vreg);
        self.next_vreg += 1;
        reg
    }
}

/// Localização no arquivo fonte preservada através do pipeline HIR → MIR → Cranelift.
///
/// `byte_offset` é preenchido pelo Cranelift após emissão do código objeto.
/// Compacta por basic block — uma entrada por bloco em vez de por instrução.
#[derive(Debug, Clone, Default)]
pub struct MIRLocation {
    /// Índice do arquivo em `OmetaWriter.sources` (ou 0 quando não disponível).
    pub file_id: u32,
    pub line: u32,
    pub column: u32,
    /// Offset no código objeto — preenchido pelo Cranelift.
    pub byte_offset: u64,
}

/// Module-level typed MIR.
#[derive(Debug, Clone, Default)]
pub struct TypedMirModule {
    pub functions: Vec<TypedMirFunction>,
}
