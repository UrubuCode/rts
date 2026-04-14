pub mod build;
pub mod cfg;
pub mod monomorphize;
pub mod typed;

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
    RuntimeEval(VReg, String),
    /// Aloca uma nova instância de classe. Cria um `RuntimeValue::Object`
    /// vazio no `ValueStore` via `FN_NEW_INSTANCE` e carrega o handle no
    /// vreg destino. O constructor é chamado separadamente pelo lowering
    /// de `Expr::New` — esta instrução apenas materializa a instância.
    NewInstance(VReg, String),
    /// Lê um campo de um objeto: `dst = obj.field`. Emite
    /// `FN_LOAD_FIELD(obj_handle, field_ptr, field_len)` → value_handle.
    LoadField(VReg, VReg, String),
    /// Escreve um campo de um objeto: `obj.field = value`. Emite
    /// `FN_STORE_FIELD(obj_handle, field_ptr, field_len, value_handle)`.
    /// Muta a `BTreeMap` do Object in-place no `ValueStore`.
    StoreField(VReg, String, VReg),
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
    /// Para cada parâmetro, `true` se o tipo anotado no HIR é numérico
    /// (`number` / `i32` / `f64` / etc.). Parâmetros numéricos são
    /// unboxed uma única vez no entry block do codegen, eliminando
    /// FN_UNBOX_NUMBER em cada uso dentro de loops. Parâmetros não-numéricos
    /// (strings, bools, objetos) permanecem como handles.
    pub param_is_numeric: Vec<bool>,
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
