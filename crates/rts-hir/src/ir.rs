/// Typed HIR — sits between the AST (rts-ast / swc_ecma_ast) and codegen.
///
/// Every node carries a `HirType` so that codegen can emit correctly-typed
/// Cranelift instructions without inspecting type annotations again.

/// Opaque identifier for a user-defined class resolved in the HIR scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ClassId(pub u32);

/// Kind of runtime handle stored in the HandleTable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum HandleKind {
    String,
    Buffer,
    Map,
    Vec,
    Function,
    Promise,
    EventEmitter,
    Regex,
    TcpListener,
    TcpStream,
    UdpSocket,
    ProcessChild,
    Mutex,
    RwLock,
    Thread,
    Opaque,
}

/// Type of every HIR value.
///
/// The codegen converts this into Cranelift `ir::Type` via `CraneliftTypeHint`.
/// The `Unknown` variant must not reach codegen — it triggers a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum HirType {
    // JS/TS native
    Number,                             // f64 — JS number semantics
    Bool,                               // i64 in ABI (0/1)
    Str,                                // two slots: ptr:i64 + len:i64
    Void,

    // Low-level integers (FFI, buffers)
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,

    // Explicit floats
    F32,
    F64,

    // SIMD: 128-bit vector lane types. Cobre i8x16/i16x8/i32x4/i64x2 +
    // f32x4/f64x2 — backend Cranelift expoe via `types::I8X16` etc.
    // Operacoes vetoriais (Splat/ExtractLane/InsertLane/aritmetica) ficam
    // em `Inst::V*` no MIR. Lowering escolhe o tipo Cranelift via
    // `VecKind`.
    V128(VecKind),

    // Compound / opaque
    Handle(HandleKind),                 // u64 opaque — HandleTable
    Array(Box<HirType>),
    Function {
        params: Vec<HirType>,
        ret: Box<HirType>,
    },
    Class(ClassId),
    Object,                             // anonymous object ({k: v})

    // Fallback
    Any,                                // triggers ABI guards at call sites
    Unknown,                            // pre-refine; must not reach codegen
}

impl HirType {
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            HirType::Number
                | HirType::I8
                | HirType::I16
                | HirType::I32
                | HirType::I64
                | HirType::I128
                | HirType::U8
                | HirType::U16
                | HirType::U32
                | HirType::U64
                | HirType::U128
                | HirType::F32
                | HirType::F64
        )
    }

    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            HirType::I8
                | HirType::I16
                | HirType::I32
                | HirType::I64
                | HirType::I128
                | HirType::U8
                | HirType::U16
                | HirType::U32
                | HirType::U64
                | HirType::U128
        )
    }

    pub fn is_float(&self) -> bool {
        matches!(self, HirType::F32 | HirType::F64 | HirType::Number)
    }
}

/// SIMD lane configuration for `HirType::V128`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum VecKind {
    I8x16,
    I16x8,
    I32x4,
    I64x2,
    F32x4,
    F64x2,
}

impl VecKind {
    pub fn lane_count(self) -> u8 {
        match self {
            VecKind::I8x16 => 16,
            VecKind::I16x8 => 8,
            VecKind::I32x4 | VecKind::F32x4 => 4,
            VecKind::I64x2 | VecKind::F64x2 => 2,
        }
    }

    pub fn is_float(self) -> bool {
        matches!(self, VecKind::F32x4 | VecKind::F64x2)
    }
}

/// Binary operators in HIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HirBinOp {
    Add, Sub, Mul, Div, Rem, Exp,
    BitAnd, BitOr, BitXor,
    Shl, Shr, UShr,
    /// Loose equality (`==` / `!=`). Distinct from `StrictEq`/`StrictNe`: the new
    /// engine needs the distinction (loose `0 == ""` is `true`, strict `false`).
    Eq, Ne,
    /// Strict equality (`===` / `!==`). swc no longer collapses these onto
    /// `Eq`/`Ne`.
    StrictEq, StrictNe,
    Lt, Le, Gt, Ge,
    LogAnd, LogOr,
    NullCoalesce,
    /// `key in obj` — own-property membership. Kept distinct from `Unsupported`
    /// (where `instanceof` still lands) so the engine can lower it to a real
    /// has-key check instead of bailing.
    In,
    /// Sentinel for an op we couldn't map (e.g. `InstanceOf`). The MIR
    /// lower bail when it sees this so the AST path keeps full ownership.
    Unsupported,
}

impl HirBinOp {
    pub fn is_arithmetic(self) -> bool {
        matches!(self, HirBinOp::Add | HirBinOp::Sub | HirBinOp::Mul | HirBinOp::Div | HirBinOp::Rem)
    }
    pub fn is_bitwise(self) -> bool {
        matches!(
            self,
            HirBinOp::BitAnd
                | HirBinOp::BitOr
                | HirBinOp::BitXor
                | HirBinOp::Shl
                | HirBinOp::Shr
                | HirBinOp::UShr
        )
    }
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            HirBinOp::Eq
                | HirBinOp::Ne
                | HirBinOp::StrictEq
                | HirBinOp::StrictNe
                | HirBinOp::Lt
                | HirBinOp::Le
                | HirBinOp::Gt
                | HirBinOp::Ge
        )
    }
}

/// Unary operators in HIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HirUnOp {
    /// Unary `-`.
    Neg,
    /// Unary `+` (ToNumber). Distinct from `Not`: swc no longer collapses `+`
    /// onto `!` — `!5` is `false`, `+5` is `5`.
    Plus,
    /// Logical `!`.
    Not,
    /// Bitwise `~`.
    BitNot,
    TypeOf,
    Void,
    /// `delete obj.prop`.
    Delete,
}

/// Literal value in HIR.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum HirLit {
    Int(i64),
    Float(f64),
    Number(f64),            // JS number (always f64 semantics)
    Bool(bool),
    Str(String),
    Null,
    Undefined,
    /// Array-literal elision (`[1, , 3]`) — the sparse HOLE element. Only ever
    /// produced inside `HirExprKind::Array`; lowers to the PolyValue hole
    /// singleton so length/`in`/join keep JS sparse semantics.
    Hole,
}

/// A single function parameter in HIR.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HirParam {
    pub name: String,
    pub ty: HirType,
    pub variadic: bool,
    pub has_default: bool,
    /// `true` for an optional param (`x?`) — omittable at the call site (value is
    /// `undefined` when not supplied). Distinct from `has_default`.
    pub optional: bool,
    /// The lowered DEFAULT initializer expr (`y = expr`), if any. The call site
    /// lowers this for an omitted trailing arg. `None` when the param has no default.
    pub default_expr: Option<Box<HirExpr>>,
}

/// HIR expression — every variant carries its resolved `HirType`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HirExpr {
    pub kind: HirExprKind,
    pub ty: HirType,
}

impl HirExpr {
    pub fn new(kind: HirExprKind, ty: HirType) -> Self {
        Self { kind, ty }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum HirExprKind {
    Lit(HirLit),
    Ident(String),

    Bin {
        op: HirBinOp,
        lhs: Box<HirExpr>,
        rhs: Box<HirExpr>,
    },
    Unary {
        op: HirUnOp,
        operand: Box<HirExpr>,
    },
    Assign {
        target: Box<HirExpr>,
        value: Box<HirExpr>,
    },
    AssignOp {
        op: HirBinOp,
        target: Box<HirExpr>,
        value: Box<HirExpr>,
    },

    // Call forms
    Call {
        callee: Box<HirExpr>,
        args: Vec<HirExpr>,
    },
    MethodCall {
        object: Box<HirExpr>,
        method: String,
        args: Vec<HirExpr>,
    },
    New {
        class: String,
        args: Vec<HirExpr>,
    },

    // Member access
    Member {
        object: Box<HirExpr>,
        prop: String,
    },
    Index {
        object: Box<HirExpr>,
        index: Box<HirExpr>,
    },

    // Constructs
    Array(Vec<HirExpr>),
    Object(Vec<(String, HirExpr)>),

    // Control-expression
    Ternary {
        cond: Box<HirExpr>,
        then: Box<HirExpr>,
        else_: Box<HirExpr>,
    },
    Await(Box<HirExpr>),

    // Type cast (narrow type annotation)
    Cast {
        expr: Box<HirExpr>,
        target: HirType,
    },

    // Arrow / fn expression
    Arrow {
        params: Vec<HirParam>,
        ret: HirType,
        body: HirArrowBody,
        /// A NAMED function expression's self-name (`function s(){ … s() … }`)
        /// — visible only inside the body. The extraction pass renames internal
        /// references to the synthesized top-level name, making self-recursive
        /// named fn-exprs liftable. `None` for arrows / anonymous fn-exprs.
        self_name: Option<String>,
        /// `async () => …` / `async function … {}` (expression or nested decl):
        /// the extraction pass lifts it to a top-level `HirFunc` with
        /// `is_async = true`, and the CALL-SITE spawn (`lower_async_spawn`)
        /// makes calls return a pending Promise — same model as a top-level
        /// `async function` declaration.
        is_async: bool,
    },

    // Pre/post increment/decrement
    PreInc(Box<HirExpr>),
    PreDec(Box<HirExpr>),
    PostInc(Box<HirExpr>),
    PostDec(Box<HirExpr>),

    /// Spread in array/call (`...x`)
    Spread(Box<HirExpr>),

    /// Sequence expression (`a, b, c`) — evaluates left to right, returns last
    Seq(Vec<HirExpr>),

    // Structured escape hatch for SWC nodes without a dedicated HIR variant.
    // Carries a text payload the engine re-interprets (regex literals,
    // `super`/`this` markers, meta-props, opt-chain) or bails on honestly.
    Raw(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum HirArrowBody {
    Expr(Box<HirExpr>),
    Block(Vec<HirStmt>),
}

/// HIR statement.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum HirStmt {
    Expr(HirExpr),
    Return(Option<HirExpr>),

    // Variable declarations
    Let {
        name: String,
        ty: HirType,
        init: Option<HirExpr>,
    },
    Const {
        name: String,
        ty: HirType,
        init: HirExpr,
    },

    // Control flow
    If {
        cond: HirExpr,
        then: Vec<HirStmt>,
        else_: Option<Vec<HirStmt>>,
    },
    While {
        cond: HirExpr,
        body: Vec<HirStmt>,
    },
    DoWhile {
        body: Vec<HirStmt>,
        cond: HirExpr,
    },
    For {
        init: Option<Box<HirStmt>>,
        cond: Option<HirExpr>,
        update: Option<HirExpr>,
        body: Vec<HirStmt>,
    },
    ForOf {
        binding: String,
        binding_ty: HirType,
        iterable: HirExpr,
        body: Vec<HirStmt>,
    },
    ForIn {
        binding: String,
        object: HirExpr,
        body: Vec<HirStmt>,
    },
    Break(Option<String>),
    Continue(Option<String>),

    // Exception
    Try {
        body: Vec<HirStmt>,
        catch: Option<HirCatch>,
        finally: Option<Vec<HirStmt>>,
    },
    Throw(HirExpr),

    // Switch
    Switch {
        discriminant: HirExpr,
        cases: Vec<HirSwitchCase>,
    },

    Block(Vec<HirStmt>),
    Labeled { label: String, body: Box<HirStmt> },

    /// Structured escape hatch — carries raw source text; the engine's stmt
    /// lowering bails on it explicitly (`unsupported!`).
    Raw(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HirCatch {
    pub binding: Option<String>,
    pub body: Vec<HirStmt>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HirSwitchCase {
    /// None = default
    pub test: Option<HirExpr>,
    pub body: Vec<HirStmt>,
}

/// A fully lowered function in HIR.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HirFunc {
    pub name: String,
    pub params: Vec<HirParam>,
    pub ret: HirType,
    pub body: Vec<HirStmt>,
    pub is_async: bool,
    pub is_arrow: bool,
}
