/// Typed HIR — sits between the AST (rts-ast / swc_ecma_ast) and codegen.
///
/// Every node carries a `HirType` so that codegen can emit correctly-typed
/// Cranelift instructions without inspecting type annotations again.

/// Opaque identifier for a user-defined class resolved in the HIR scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassId(pub u32);

/// Kind of runtime handle stored in the HandleTable.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Binary operators in HIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirBinOp {
    Add, Sub, Mul, Div, Rem,
    BitAnd, BitOr, BitXor,
    Shl, Shr, UShr,
    Eq, Ne, Lt, Le, Gt, Ge,
    LogAnd, LogOr,
    NullCoalesce,
}

impl HirBinOp {
    pub fn is_arithmetic(self) -> bool {
        matches!(self, HirBinOp::Add | HirBinOp::Sub | HirBinOp::Mul | HirBinOp::Div | HirBinOp::Rem)
    }
    pub fn is_comparison(self) -> bool {
        matches!(self, HirBinOp::Eq | HirBinOp::Ne | HirBinOp::Lt | HirBinOp::Le | HirBinOp::Gt | HirBinOp::Ge)
    }
}

/// Unary operators in HIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirUnOp {
    Neg,
    Not,
    BitNot,
    TypeOf,
    Void,
}

/// Literal value in HIR.
#[derive(Debug, Clone)]
pub enum HirLit {
    Int(i64),
    Float(f64),
    Number(f64),            // JS number (always f64 semantics)
    Bool(bool),
    Str(String),
    Null,
    Undefined,
}

/// A single function parameter in HIR.
#[derive(Debug, Clone)]
pub struct HirParam {
    pub name: String,
    pub ty: HirType,
    pub variadic: bool,
    pub has_default: bool,
}

/// HIR expression — every variant carries its resolved `HirType`.
#[derive(Debug, Clone)]
pub struct HirExpr {
    pub kind: HirExprKind,
    pub ty: HirType,
}

impl HirExpr {
    pub fn new(kind: HirExprKind, ty: HirType) -> Self {
        Self { kind, ty }
    }
}

#[derive(Debug, Clone)]
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

    // Fallback for unrecognised SWC nodes that rts-codegen handles via its
    // own legacy path. Carries the raw text so the old codegen can fall back.
    Raw(String),
}

#[derive(Debug, Clone)]
pub enum HirArrowBody {
    Expr(Box<HirExpr>),
    Block(Vec<HirStmt>),
}

/// HIR statement.
#[derive(Debug, Clone)]
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

    /// Fallback — carries raw source text for legacy codegen path.
    Raw(String),
}

#[derive(Debug, Clone)]
pub struct HirCatch {
    pub binding: Option<String>,
    pub body: Vec<HirStmt>,
}

#[derive(Debug, Clone)]
pub struct HirSwitchCase {
    /// None = default
    pub test: Option<HirExpr>,
    pub body: Vec<HirStmt>,
}

/// A fully lowered function in HIR.
#[derive(Debug, Clone)]
pub struct HirFunc {
    pub name: String,
    pub params: Vec<HirParam>,
    pub ret: HirType,
    pub body: Vec<HirStmt>,
    pub is_async: bool,
    pub is_arrow: bool,
}
