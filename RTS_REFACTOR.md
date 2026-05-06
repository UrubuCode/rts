# RTS — Plano de Refatoração em Workspace de Crates

**Baseado em:** análise do PerryTS + estado atual do RTS + Cranelift 0.131 API  
**Objetivo:** separar o monólito `src/` em crates independentes, introduzir HIR e
MIR como camadas explícitas entre parser e codegen, manter ABI `extern "C"` intacta,
e usar o repertório completo do Cranelift corretamente em cada camada.

---

## Por que fazer isso

| Problema atual | Consequência |
|---|---|
| `codegen/lower/func.rs` com 9 k linhas | revisão difícil, build lento, testes acoplados |
| Parser → Cranelift direto (sem IR) | otimizações de alto nível impossíveis — Cranelift não faz inlining, DCE, escape analysis, constant folding interprocessual |
| `AbiType` acoplada a `cranelift_codegen::ir::Type` em `rts-abi` | qualquer crate que precise dos contratos ABI puxa cranelift como dep |
| Todos os 40+ namespaces no mesmo crate | recompilar tudo ao mexer em qualquer namespace |
| `type_system/` com 4 arquivos (~500 linhas) | type refine agressivo inviável — emite `any`-path com coerções em hot loops |
| `CallConv::Tail` não usado em todos os lugares certos | tail calls não otimizados em casos que poderiam ser |
| Narrow types (i8, i16, i32) sem suporte de primeira classe no TS | blocos de baixo nível (WASM, FFI, buffers) precisam de bitmasking manual hoje |

Perry resolveu isso com 30+ crates. RTS não precisa chegar lá, mas ~10 crates
resolve 80% dos problemas sem overhead de manutenção.

---

## Mapa de Crates Proposto

```
rts/                          (workspace root — Cargo.toml de workspace)
  crates/
    rts-ast/                  — AST público (hoje src/parser/ast.rs)
    rts-parser/               — SWC wrapper + lowering pra AST (hoje src/parser/)
    rts-hir/                  — HIR tipado: Expr/Stmt/Type + analysis + lowering de AST
    rts-mir/                  — MIR SSA-like: blocos básicos + passes de otimização
    rts-abi/                  — contrato ABI (hoje src/abi/) — ZERO deps Cranelift
    rts-codegen/              — Cranelift backend: consome MIR, emite Cranelift IR
    rts-runtime/              — todos os namespaces (hoje src/namespaces/)
    rts-linker/               — linker wrapper (hoje src/linker/)
    rts-diagnostics/          — erros estruturados (hoje src/diagnostics/)
    rts-cli/                  — CLI + pipeline (hoje src/cli/ + src/pipeline.rs)
  src/
    main.rs                   — entrypoint (apenas `rts_cli::main()`)
```

### Grafo de dependências (sem ciclos)

```
rts-diagnostics  (sem deps RTS)
rts-ast          (sem deps RTS)
    ↑
rts-parser       (swc_*, rts-ast, rts-diagnostics)
    ↑
rts-hir          (rts-ast, rts-abi, rts-diagnostics)
    ↑
rts-mir          (rts-hir, rts-abi)
    ↑
rts-codegen      (rts-mir, rts-abi, cranelift-*)   ← ÚNICA crate que depende de cranelift
    ↑
rts-cli          (rts-codegen, rts-parser, rts-linker, rts-runtime, rts-abi)
    ↑
rts (bin)        (rts-cli)

rts-abi          (sem deps cranelift — só tipos e símbolos próprios)
rts-runtime      (rts-abi, tokio, rayon, rustls, ...)
rts-linker       (rts-abi, object, target-lexicon)
```

**Invariante crítica:** somente `rts-codegen` importa `cranelift-*`. Qualquer
`use cranelift_codegen` fora desse crate é um erro de arquitetura.

---

## Crate por Crate — Escopo, Conteúdo e Lógica Cranelift

---

### `rts-ast`

AST pura sem lógica de parse ou codegen.

**Mover de:** `src/parser/ast.rs`

```
rts-ast/src/
  lib.rs
  expr.rs        — Expr, Lit, BinOp, UnOp
  stmt.rs        — Stmt, Decl
  item.rs        — Item::Function, Item::Class, Item::Import, ...
  types.rs       — TsType, TsAnnotation (com suporte a i8/i16/i32/i64/u8/u16/u32/u64/f32/f64)
  span.rs        — SourceSpan, SourceFile
```

#### Extensão de tipos primitivos (suporte a baixo nível)

O `TsType` precisa cobrir todos os inteiros que o Cranelift suporta nativamente:

```rust
pub enum TsPrimitive {
    // JS/TS nativos
    Number,     // f64 semântico JS
    Boolean,
    String,
    // Extensões de baixo nível (para FFI, buffers, WASM)
    I8, I16, I32, I64, I128,
    U8, U16, U32, U64, U128,
    F32, F64,
    // Void e never
    Void,
    Never,
}
```

Cranelift tem suporte nativo para todos esses via `types::I8` até `types::I128`,
`types::F32`, `types::F64` — mapeamento direto sem overhead.

Deps: apenas `serde` (feature flag).

---

### `rts-parser`

SWC wrapper + lowering para `rts-ast`. Hoje em `src/parser/`.

**Mover de:** `src/parser/*.rs`

```
rts-parser/src/
  lib.rs
  parse_api.rs          — entrada pública: parse_file / parse_str
  lowering_items.rs     — SWC AST → rts-ast Items
  lowering_decls.rs     — declarações
  lowering_helpers.rs   — helpers
  generator_desugar.rs  — desugar generators
  passes/
    expand_async.rs     — expand_async_functions
    purity_pass.rs      — purity_pass silent parallelism
    reduce_pass.rs      — reduce_pass
    array_methods.rs    — array_methods_pass
```

Os 3 passes de silent parallelism operam sobre AST antes do HIR — sem deps Cranelift.

---

### `rts-hir` ⭐ (nova camada)

IR tipado entre AST e codegen. Cada nó HIR carrega seu `HirType` — resolve
ambiguidade entre `number` JS (f64) e inteiros nativos, eliminando `any`-paths
em hot loops.

**Criar do zero.**

```
rts-hir/src/
  lib.rs
  ir.rs           — HirExpr, HirStmt, HirFunc, HirType, HirLit
  lower.rs        — rts-ast → HIR
  type_refine.rs  — refine_type_from_init
  type_map.rs     — HirType → CraneliftTypeHint (usado por rts-codegen)
  analysis.rs     — análise de escopo, resolução de nomes
  walker.rs       — visitor sobre HIR
```

#### `HirType` — cobrindo toda a grade de tipos do Cranelift

```rust
/// Tipo de valor no HIR. Mapeamento com Cranelift em type_map.rs.
pub enum HirType {
    // JS/TS nativos
    Number,                         // f64 — semântica JS
    Bool,                           // i64 no ABI (0/1); i8 internamente em icmp
    Str,                            // StrPtr (ptr: i64 + len: i64)
    Void,

    // Inteiros explícitos (baixo nível / FFI)
    I8, I16, I32, I64, I128,
    U8, U16, U32, U64, U128,

    // Floats explícitos
    F32, F64,

    // Compostos
    Handle(HandleKind),             // u64 opaco — HandleTable
    Array(Box<HirType>),
    Function { params: Vec<HirType>, ret: Box<HirType> },
    Class(ClassId),

    // Fallback
    Any,                            // gatilha guards ABI em call sites
    Unknown,                        // pré-refine; erro se chegar ao codegen
}
```

#### `type_map.rs` — bridge HIR → Cranelift (vive em `rts-hir`, não em `rts-codegen`)

Este módulo exporta um enum leve (`CraneliftTypeHint`) que `rts-codegen` converte
em `cranelift_codegen::ir::Type`. Isso mantém `rts-hir` sem dep de cranelift
enquanto o codegen ainda tem informação de tipo rica:

```rust
/// Hint de tipo para codegen — não usa tipos cranelift diretamente.
pub enum CraneliftTypeHint {
    I8, I16, I32, I64, I128,
    F32, F64,
    Ptr,      // pointer width (I32 ou I64 conforme alvo)
    Void,
}

impl HirType {
    pub fn cranelift_hint(&self) -> Option<CraneliftTypeHint> {
        match self {
            HirType::I8            => Some(CraneliftTypeHint::I8),
            HirType::I16           => Some(CraneliftTypeHint::I16),
            HirType::I32 | HirType::U32 => Some(CraneliftTypeHint::I32),
            HirType::I64 | HirType::U64
            | HirType::Number      // f64 bits viajam como i64 no ABI
            | HirType::Bool
            | HirType::Handle(_)   => Some(CraneliftTypeHint::I64),
            HirType::I128 | HirType::U128 => Some(CraneliftTypeHint::I128),
            HirType::F32           => Some(CraneliftTypeHint::F32),
            HirType::F64           => Some(CraneliftTypeHint::F64),
            HirType::Void          => Some(CraneliftTypeHint::Void),
            HirType::Str           => None, // dois slots: ptr + len
            HirType::Any
            | HirType::Unknown     => None, // resolve em runtime
            _ => None,
        }
    }
}
```

#### `refine_type_from_init` — port do Perry + extensão para narrow types

```rust
pub fn refine_type_from_init(expr: &HirExpr, scope: &Scope) -> HirType {
    match expr {
        // Literais
        HirExpr::Lit(HirLit::Int(_))    => HirType::I64,     // inteiro sem anotação → i64
        HirExpr::Lit(HirLit::Float(_))  => HirType::F64,     // float sem anotação → f64
        HirExpr::Lit(HirLit::Number(_)) => HirType::Number,  // JS number → f64 semântico
        HirExpr::Lit(HirLit::Bool(_))   => HirType::Bool,
        HirExpr::Lit(HirLit::Str(_))    => HirType::Str,

        // Aritmética numérica — propaga tipo se ambos os lados são numéricos
        HirExpr::Bin { op, lhs, rhs } if op.is_arithmetic() => {
            let lt = refine_type_from_init(lhs, scope);
            let rt = refine_type_from_init(rhs, scope);
            numeric_promotion(lt, rt)   // ex: i32 op f64 → f64
        }

        // Arrays e coleções
        HirExpr::Array(_) => HirType::Array(Box::new(HirType::Any)),
        HirExpr::New { class, .. } if class == "Array" =>
            HirType::Array(Box::new(HirType::Any)),
        HirExpr::New { class, .. } =>
            scope.resolve_class(class)
                 .map(HirType::Class)
                 .unwrap_or(HirType::Unknown),

        // Calls — usar tipo de retorno da assinatura conhecida
        HirExpr::Call { callee, .. } =>
            scope.return_type_of(callee).unwrap_or(HirType::Unknown),

        // Cast explícito de tipo primitivo
        HirExpr::Cast { target, .. } => target.clone(),

        _ => HirType::Unknown,
    }
}

/// Promoção numérica entre dois tipos (segue regras C/Rust)
fn numeric_promotion(a: HirType, b: HirType) -> HirType {
    use HirType::*;
    match (a, b) {
        (F64, _) | (_, F64)   => F64,
        (F32, _) | (_, F32)   => F32,
        (I128, _) | (_, I128)
        | (U128, _) | (_, U128) => I128,
        (I64, _) | (_, I64)
        | (U64, _) | (_, U64) => I64,
        (I32, _) | (_, I32)   => I32,
        (I16, _) | (_, I16)   => I16,
        (I8, _)  | (_, I8)    => I8,
        (Number, _) | (_, Number) => Number,
        _ => I64, // fallback seguro
    }
}
```

**Impacto:** `let i = 0; for(...) i += 1` hoje emite `any`-path com coerções.
Com refine, `i` vira `I64` e o codegen emite `iadd_imm` direto — sem call extern.

---

### `rts-mir` ⭐ (nova camada)

MIR SSA-like — blocos básicos com block parameters (= phis), operações tipadas.
Espelha o modelo do Cranelift (`Block` + params + `Terminator`) para que a
tradução MIR → Cranelift IR seja mecânica e sem surpresas.

**Criar do zero.**

```
rts-mir/src/
  lib.rs
  ir.rs           — MirFunc, BasicBlock, Inst, Value, Terminator
  lower.rs        — HIR → MIR
  passes/
    dce.rs        — dead code elimination
    fold.rs       — constant folding + strength reduction
    inline.rs     — inlining de fns pequenas (future)
    escape.rs     — escape analysis para handles (future)
    narrow.rs     — canonicalização de narrow types (i8/i16/i32 ops)
  verify.rs       — invariantes MIR (debug build)
```

#### Estrutura MIR — espelhando o modelo de blocos do Cranelift

```rust
pub type ValueId = u32;
pub type BlockId = u32;

pub struct MirFunc {
    pub name:   String,
    pub conv:   CallConvHint,           // Tail | SystemV | WindowsFastcall
    pub params: Vec<(ValueId, HirType)>,
    pub ret:    HirType,
    pub blocks: Vec<BasicBlock>,
    pub values: Vec<HirType>,           // type table indexed by ValueId
}

pub struct BasicBlock {
    pub id:     BlockId,
    pub params: Vec<(ValueId, HirType)>, // block params = SSA phis
    pub insts:  Vec<Inst>,
    pub term:   Terminator,
}

pub enum Inst {
    // Arithmetic
    IAdd  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    IAddImm { dst: ValueId, lhs: ValueId, imm: i64 },
    ISub  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    IMul  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    IMulImm { dst: ValueId, lhs: ValueId, imm: i64 },
    SDiv  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    UDiv  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    SRem  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    URem  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    INeg  { dst: ValueId, src: ValueId },

    // Bitwise
    BAnd  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    BAndImm { dst: ValueId, lhs: ValueId, imm: i64 },
    BOr   { dst: ValueId, lhs: ValueId, rhs: ValueId },
    BOrImm  { dst: ValueId, lhs: ValueId, imm: i64 },
    BXor  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    BXorImm { dst: ValueId, lhs: ValueId, imm: i64 },
    BNot  { dst: ValueId, src: ValueId },

    // Shifts
    IShl  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    IShlImm { dst: ValueId, lhs: ValueId, imm: i64 },
    UShr  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    SShr  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    SShrImm { dst: ValueId, lhs: ValueId, imm: i64 },
    Rotl  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    Rotr  { dst: ValueId, lhs: ValueId, rhs: ValueId },

    // Bit counting
    Clz    { dst: ValueId, src: ValueId },
    Ctz    { dst: ValueId, src: ValueId },
    Popcnt { dst: ValueId, src: ValueId },
    Bswap  { dst: ValueId, src: ValueId },

    // Float arithmetic
    FAdd  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    FSub  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    FMul  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    FDiv  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    FNeg  { dst: ValueId, src: ValueId },
    FAbs  { dst: ValueId, src: ValueId },
    Sqrt  { dst: ValueId, src: ValueId },
    Fma   { dst: ValueId, a: ValueId, b: ValueId, c: ValueId },
    FMin  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    FMax  { dst: ValueId, lhs: ValueId, rhs: ValueId },
    Ceil  { dst: ValueId, src: ValueId },
    Floor { dst: ValueId, src: ValueId },
    Trunc { dst: ValueId, src: ValueId },

    // Conversions
    IReduce  { dst: ValueId, src: ValueId, to: HirType }, // truncate
    UExtend  { dst: ValueId, src: ValueId, to: HirType }, // zero-extend
    SExtend  { dst: ValueId, src: ValueId, to: HirType }, // sign-extend
    FPromote { dst: ValueId, src: ValueId },               // f32 → f64
    FDemote  { dst: ValueId, src: ValueId },               // f64 → f32
    CvtFromSint { dst: ValueId, src: ValueId, to: HirType }, // int → float
    CvtFromUint { dst: ValueId, src: ValueId, to: HirType },
    CvtToSintSat { dst: ValueId, src: ValueId, to: HirType }, // float → int (sat)
    CvtToUintSat { dst: ValueId, src: ValueId, to: HirType },
    Bitcast  { dst: ValueId, src: ValueId, to: HirType }, // reinterpret bits

    // Comparisons
    ICmp { dst: ValueId, cond: IntCond, lhs: ValueId, rhs: ValueId },
    FCmp { dst: ValueId, cond: FloatCond, lhs: ValueId, rhs: ValueId },

    // Constants
    IConst { dst: ValueId, ty: HirType, val: i64 },
    F32Const { dst: ValueId, val: f32 },
    F64Const { dst: ValueId, val: f64 },

    // Memory
    Load  { dst: ValueId, ty: HirType, ptr: ValueId, offset: i32, flags: MemHint },
    Store { val: ValueId, ptr: ValueId, offset: i32, flags: MemHint },
    // Narrow loads
    ULoad8  { dst: ValueId, ptr: ValueId, offset: i32 },
    ULoad16 { dst: ValueId, ptr: ValueId, offset: i32 },
    ULoad32 { dst: ValueId, ptr: ValueId, offset: i32 },
    SLoad8  { dst: ValueId, ptr: ValueId, offset: i32 },
    SLoad16 { dst: ValueId, ptr: ValueId, offset: i32 },
    SLoad32 { dst: ValueId, ptr: ValueId, offset: i32 },
    // Narrow stores
    IStore8  { val: ValueId, ptr: ValueId, offset: i32 },
    IStore16 { val: ValueId, ptr: ValueId, offset: i32 },
    IStore32 { val: ValueId, ptr: ValueId, offset: i32 },

    // Stack
    StackAlloc { dst: ValueId, size: u32, align: u8 }, // → stack slot addr

    // Branchless select
    Select { dst: ValueId, cond: ValueId, on_true: ValueId, on_false: ValueId },

    // Extern call (runtime ABI)
    CallExtern { dst: Option<ValueId>, sym: String, args: Vec<ValueId> },

    // Atomics
    AtomicLoad  { dst: ValueId, ptr: ValueId, order: MemOrder },
    AtomicStore { val: ValueId, ptr: ValueId, order: MemOrder },
    AtomicRmw   { dst: ValueId, op: RmwOp, ptr: ValueId, val: ValueId, order: MemOrder },
    AtomicCas   { dst: ValueId, ptr: ValueId, expected: ValueId, replacement: ValueId, order: MemOrder },
    Fence       { order: MemOrder },

    // GC
    DeclareGcValue { val: ValueId }, // marks value as needing stack map entry
}

pub enum Terminator {
    Return(Vec<ValueId>),
    Jump  { target: BlockId, args: Vec<ValueId> },
    Brif  { cond: ValueId, then_block: BlockId, then_args: Vec<ValueId>,
                           else_block: BlockId, else_args: Vec<ValueId> },
    Switch { index: ValueId, default: BlockId, cases: Vec<(u64, BlockId)> },
    TailCall  { sym: String, args: Vec<ValueId> },
    TailCallIndirect { fn_ptr: ValueId, args: Vec<ValueId> },
    Trap  { code: TrapHint },
}

/// Condições inteiras — espelha IntCC do Cranelift exatamente
pub enum IntCond {
    Eq, Ne,
    Slt, Sle, Sgt, Sge,  // signed
    Ult, Ule, Ugt, Uge,  // unsigned
}

/// Condições float — espelha FloatCC do Cranelift
pub enum FloatCond {
    Eq, Ne,
    OLt, OLe, OGt, OGe, ONe,   // ordered (false if NaN)
    ULt, ULe, UGt, UGe, UNe,   // unordered (true if NaN)
    Ordered, Unordered,          // NaN check
}

pub enum MemHint { Trusted, Notrap, Default }
pub enum MemOrder { Relaxed, Acquire, Release, AcqRel, SeqCst }
pub enum RmwOp { Add, Sub, And, Or, Xor, Nand, Xchg, Umin, Umax, Smin, Smax }
pub enum TrapHint { DivByZero, IntOverflow, HeapOob, StackOverflow, User(u16) }
pub enum CallConvHint { Tail, SystemV, WindowsFastcall, Cold }
```

**MIR é isomórfico ao Cranelift IR** — cada `Inst` mapeia para exatamente uma
família de instruções Cranelift. Isso torna a tradução em `rts-codegen` mecânica
e testável independentemente do backend.

#### `passes/fold.rs` — strength reduction + constant folding

Passes que o Cranelift **não** faz automaticamente (exceto com `use_egraphs`, que
é intraprocessual). O MIR faz interprocessual:

```rust
// Multiplicação por potência de 2 → shift
IConst { val: imm, .. } if is_power_of_two(imm as u64) =>
    IMulImm → IShlImm(lhs, imm.trailing_zeros() as i64)

// Módulo por potência de 2 → máscara (unsigned)
IConst { val: imm, .. } if is_power_of_two(imm as u64) =>
    URem → BAndImm(lhs, imm - 1)

// Divisão por potência de 2 → shift aritmético (signed)
IConst { val: imm, .. } if is_power_of_two(imm as u64) =>
    SDiv → SShrImm(lhs, imm.trailing_zeros() as i64)

// Constant folding puro
IAdd(IConst(a), IConst(b)) → IConst(a + b)
IMul(IConst(a), IConst(b)) → IConst(a * b)
// etc.
```

#### `passes/narrow.rs` — canonicalização de narrow types

Operações em i8/i16/i32 precisam de tratamento especial porque Cranelift
não tem aritmética nativa de 8/16 bits — opera em i32/i64 com masks:

```rust
// Após operação em I8: resultado mascarado pra 8 bits
IAdd { ty: I8, .. } =>
    IAdd(lhs, rhs) seguido de BAndImm(result, 0xFF)

// Leitura de i8 de memória: uload8 + zext (já feito automaticamente)
// Escrita de i8: ireduce(I8, val) implícito em istore8

// Promoção para aritmética: I8 operand → sextend I64 antes de IAdd
// Resultado: ireduce I8 depois
```

Isso garante semântica correta de overflow para tipos menores sem surpresas.

---

### `rts-abi`

Extração de `src/abi/` **sem qualquer dep de cranelift**. Zero `cranelift-*` no
`Cargo.toml` deste crate.

**Mover de:** `src/abi/`

```
rts-abi/src/
  lib.rs
  member.rs         — NamespaceSpec, NamespaceMember, Intrinsic, MemberKind
  types.rs          — AbiType (own enum, não usa cranelift ir::Type)
  signature.rs      — LoweredSignature (structs próprias, sem cranelift)
  symbols.rs        — convenção __RTS_* + rts_sym! macro
  guards.rs         — guard_for()
  global_class.rs   — GlobalClassSpec, GLOBAL_CLASS_SPECS
  handles.rs        — HandleTable ABI constants
  mod.rs            — SPECS + lookup() + global_class_lookup()
```

#### `AbiType` — desacoplada de `cranelift_codegen::ir::Type`

```rust
/// Tipo ABI RTS — sem dep de cranelift.
/// Conversão para cranelift::ir::Type fica em rts-codegen/src/abi_bridge.rs
pub enum AbiType {
    Void,
    Bool,       // i64 no boundary extern "C" (0/1)
    I8, I16, I32, I64, I128,
    U8, U16, U32, U64,
    F32, F64,
    StrPtr,     // 2 slots: (ptr: i64, len: i64)
    Handle,     // u64 opaco — HandleTable
}
```

#### `LoweredSignature` — sem tipos cranelift

```rust
pub struct LoweredSignature {
    pub symbol:  &'static str,
    pub params:  &'static [AbiType],
    pub returns: AbiType,
    pub conv:    AbiCallConv,
}

pub enum AbiCallConv { Tail, SystemV, WindowsFastcall, Cold }
```

A conversão `LoweredSignature → cranelift::ir::Signature` fica em
`rts-codegen/src/abi_bridge.rs` — único lugar que conhece os dois mundos.

---

### `rts-codegen`

Backend Cranelift. **Único crate que importa `cranelift-*`.**
Consome `MirFunc` e emite Cranelift IR via `FunctionBuilder`.

**Mover de:** `src/codegen/`

```
rts-codegen/src/
  lib.rs
  setup.rs          — ISA + settings (opt_level, preserve_frame_pointers, use_egraphs)
  jit.rs            — JITModule emitter + register_all_symbols
  emit.rs           — ObjectModule emitter (AOT)
  abi_bridge.rs     — AbiType/LoweredSignature → cranelift ir::Type / ir::Signature
  lower/
    func.rs         — MirFunc → Cranelift function (instancia FunctionBuilder)
    inst.rs         — Inst → builder.ins().*  (tradução mecânica 1:1)
    term.rs         — Terminator → jump/brif/return/return_call
    ctx.rs          — FnCtx: builder + module + symbol cache
    intrinsics.rs   — Intrinsic → Cranelift IR inline (sqrt, abs, min, max, etc.)
    gc.rs           — declare_value_needs_stack_map + stack map registry
```

#### `setup.rs` — configuração correta do ISA

```rust
pub fn build_isa(opt: OptLevel) -> Arc<dyn TargetIsa> {
    let mut b = settings::builder();

    // Otimizador principal — e-graphs fazem constant folding, CSE, identidades
    b.set("use_egraphs", "true").unwrap();
    b.set("enable_alias_analysis", "true").unwrap();
    b.set("enable_jump_tables", "true").unwrap();

    match opt {
        OptLevel::None  => b.set("opt_level", "none").unwrap(),
        OptLevel::Speed => b.set("opt_level", "speed").unwrap(),
        OptLevel::Size  => b.set("opt_level", "speed_and_size").unwrap(),
    }

    // Obrigatório para tail calls em x86-64 (return_call usa frame pointer)
    b.set("preserve_frame_pointers", "true").unwrap();

    cranelift_native::builder()
        .unwrap()
        .finish(settings::Flags::new(b))
        .unwrap()
}
```

#### `abi_bridge.rs` — tradução AbiType → cranelift ir::Type

```rust
use cranelift_codegen::ir::{types, Type, AbiParam, Signature};
use cranelift_codegen::isa::CallConv;
use rts_abi::{AbiType, AbiCallConv, LoweredSignature};

pub fn abi_type_to_cl(ty: &AbiType, ptr_ty: Type) -> Vec<Type> {
    match ty {
        AbiType::Void    => vec![],
        AbiType::Bool    => vec![types::I64],   // 0/1 em i64 no boundary
        AbiType::I8      => vec![types::I8],
        AbiType::I16     => vec![types::I16],
        AbiType::I32     => vec![types::I32],
        AbiType::I64     => vec![types::I64],
        AbiType::I128    => vec![types::I128],
        AbiType::U8      => vec![types::I8],    // sem unsig nado no CL — instrução decide
        AbiType::U16     => vec![types::I16],
        AbiType::U32     => vec![types::I32],
        AbiType::U64     => vec![types::I64],
        AbiType::F32     => vec![types::F32],
        AbiType::F64     => vec![types::F64],
        AbiType::StrPtr  => vec![types::I64, types::I64],  // ptr + len
        AbiType::Handle  => vec![types::I64],   // u64 como i64
    }
}

pub fn lowered_to_cl_sig(sig: &LoweredSignature, ptr_ty: Type) -> Signature {
    let conv = match sig.conv {
        AbiCallConv::Tail             => CallConv::Tail,
        AbiCallConv::SystemV          => CallConv::SystemV,
        AbiCallConv::WindowsFastcall  => CallConv::WindowsFastcall,
        AbiCallConv::Cold             => CallConv::Cold,
    };
    let mut cl_sig = Signature::new(conv);
    for p in sig.params {
        for ty in abi_type_to_cl(p, ptr_ty) {
            cl_sig.params.push(AbiParam::new(ty));
        }
    }
    for ty in abi_type_to_cl(&sig.returns, ptr_ty) {
        cl_sig.returns.push(AbiParam::new(ty));
    }
    cl_sig
}
```

#### `lower/inst.rs` — tradução mecânica Inst → `builder.ins().*`

Cada variante de `Inst` tem exatamente um destino no Cranelift. Exemplos:

```rust
use cranelift_codegen::ir::{types, InstBuilder, condcodes::{IntCC, FloatCC}, MemFlags};

pub fn lower_inst(inst: &Inst, ctx: &mut FnCtx) {
    match inst {
        // Aritmética inteira
        Inst::IAdd { dst, lhs, rhs } => {
            let v = ctx.builder.ins().iadd(ctx.val(*lhs), ctx.val(*rhs));
            ctx.bind(*dst, v);
        }
        Inst::IAddImm { dst, lhs, imm } => {
            let v = ctx.builder.ins().iadd_imm(ctx.val(*lhs), *imm);
            ctx.bind(*dst, v);
        }
        Inst::IShlImm { dst, lhs, imm } => {
            let v = ctx.builder.ins().ishl_imm(ctx.val(*lhs), *imm);
            ctx.bind(*dst, v);
        }
        Inst::SShrImm { dst, lhs, imm } => {
            let v = ctx.builder.ins().sshr_imm(ctx.val(*lhs), *imm);
            ctx.bind(*dst, v);
        }
        Inst::BAndImm { dst, lhs, imm } => {
            let v = ctx.builder.ins().band_imm(ctx.val(*lhs), *imm);
            ctx.bind(*dst, v);
        }

        // Conversões de tipo
        Inst::IReduce { dst, src, to } => {
            let cl_ty = ctx.hir_to_cl(to);
            let v = ctx.builder.ins().ireduce(cl_ty, ctx.val(*src));
            ctx.bind(*dst, v);
        }
        Inst::UExtend { dst, src, to } => {
            let cl_ty = ctx.hir_to_cl(to);
            let v = ctx.builder.ins().uextend(cl_ty, ctx.val(*src));
            ctx.bind(*dst, v);
        }
        Inst::SExtend { dst, src, to } => {
            let cl_ty = ctx.hir_to_cl(to);
            let v = ctx.builder.ins().sextend(cl_ty, ctx.val(*src));
            ctx.bind(*dst, v);
        }
        Inst::FPromote { dst, src } => {
            let v = ctx.builder.ins().fpromote(types::F64, ctx.val(*src));
            ctx.bind(*dst, v);
        }
        Inst::FDemote { dst, src } => {
            let v = ctx.builder.ins().fdemote(types::F32, ctx.val(*src));
            ctx.bind(*dst, v);
        }
        Inst::CvtFromSint { dst, src, to } => {
            let cl_ty = ctx.hir_to_cl(to);
            let v = ctx.builder.ins().fcvt_from_sint(cl_ty, ctx.val(*src));
            ctx.bind(*dst, v);
        }
        Inst::CvtToSintSat { dst, src, to } => {
            let cl_ty = ctx.hir_to_cl(to);
            let v = ctx.builder.ins().fcvt_to_sint_sat(cl_ty, ctx.val(*src));
            ctx.bind(*dst, v);
        }
        Inst::Bitcast { dst, src, to } => {
            let cl_ty = ctx.hir_to_cl(to);
            let v = ctx.builder.ins().bitcast(cl_ty, MemFlags::new(), ctx.val(*src));
            ctx.bind(*dst, v);
        }

        // Comparações — icmp retorna I8; uextend para I64 se necessário para ABI
        Inst::ICmp { dst, cond, lhs, rhs } => {
            let cc = int_cond_to_cl(cond);
            let cmp = ctx.builder.ins().icmp(cc, ctx.val(*lhs), ctx.val(*rhs));
            // extend para I64 para uso como valor ABI (bool retornado como i64)
            let v = ctx.builder.ins().uextend(types::I64, cmp);
            ctx.bind(*dst, v);
        }
        Inst::FCmp { dst, cond, lhs, rhs } => {
            let cc = float_cond_to_cl(cond);
            let cmp = ctx.builder.ins().fcmp(cc, ctx.val(*lhs), ctx.val(*rhs));
            let v = ctx.builder.ins().uextend(types::I64, cmp);
            ctx.bind(*dst, v);
        }

        // Narrow loads/stores
        Inst::ULoad8  { dst, ptr, offset } => {
            let v = ctx.builder.ins().uload8(MemFlags::trusted(), ctx.val(*ptr), *offset);
            ctx.bind(*dst, v);
        }
        Inst::SLoad16 { dst, ptr, offset } => {
            let v = ctx.builder.ins().sload16(MemFlags::trusted(), ctx.val(*ptr), *offset);
            ctx.bind(*dst, v);
        }
        Inst::IStore8  { val, ptr, offset } => {
            ctx.builder.ins().istore8(MemFlags::trusted(), ctx.val(*val), ctx.val(*ptr), *offset);
        }
        Inst::IStore32 { val, ptr, offset } => {
            ctx.builder.ins().istore32(MemFlags::trusted(), ctx.val(*val), ctx.val(*ptr), *offset);
        }

        // Branchless select
        Inst::Select { dst, cond, on_true, on_false } => {
            let v = ctx.builder.ins().select(
                ctx.val(*cond), ctx.val(*on_true), ctx.val(*on_false)
            );
            ctx.bind(*dst, v);
        }

        // Atomics
        Inst::AtomicRmw { dst, op, ptr, val, order } => {
            let cl_op  = rmw_op_to_cl(op);
            let cl_ord = mem_order_to_cl(order);
            let v = ctx.builder.ins().atomic_rmw(cl_op, cl_ord, ctx.val(*ptr), ctx.val(*val));
            ctx.bind(*dst, v);
        }
        Inst::AtomicCas { dst, ptr, expected, replacement, order } => {
            let cl_ord = mem_order_to_cl(order);
            let v = ctx.builder.ins().atomic_cas(
                cl_ord, ctx.val(*ptr), ctx.val(*expected), ctx.val(*replacement)
            );
            ctx.bind(*dst, v);
        }
        Inst::Fence { order } => {
            ctx.builder.ins().fence(mem_order_to_cl(order));
        }

        // GC stack map
        Inst::DeclareGcValue { val } => {
            ctx.builder.declare_value_needs_stack_map(ctx.val(*val));
        }

        // ... demais variantes seguem o mesmo padrão
    }
}
```

#### `lower/term.rs` — Terminator → terminators do Cranelift

```rust
pub fn lower_term(term: &Terminator, ctx: &mut FnCtx) {
    match term {
        Terminator::Return(vals) => {
            let cl_vals: Vec<_> = vals.iter().map(|v| ctx.val(*v)).collect();
            ctx.builder.ins().return_(&cl_vals);
        }
        Terminator::Jump { target, args } => {
            let cl_block = ctx.block(*target);
            let cl_args: Vec<_> = args.iter().map(|v| ctx.val(*v)).collect();
            ctx.builder.ins().jump(cl_block, &cl_args);
        }
        Terminator::Brif { cond, then_block, then_args, else_block, else_args } => {
            let cl_then = ctx.block(*then_block);
            let cl_else = ctx.block(*else_block);
            let cl_then_args: Vec<_> = then_args.iter().map(|v| ctx.val(*v)).collect();
            let cl_else_args: Vec<_> = else_args.iter().map(|v| ctx.val(*v)).collect();
            ctx.builder.ins().brif(
                ctx.val(*cond),
                cl_then, &cl_then_args,
                cl_else, &cl_else_args,
            );
        }
        Terminator::Switch { index, default, cases } => {
            // Usa Switch builder do cranelift_frontend — backend escolhe br_table vs busca binária
            let mut sw = cranelift_frontend::Switch::new();
            for &(key, blk) in cases {
                sw.set_entry(key, ctx.block(blk));
            }
            sw.emit(&mut ctx.builder, ctx.val(*index), ctx.block(*default));
        }
        Terminator::TailCall { sym, args } => {
            let fref = ctx.get_func_ref(sym);
            let cl_args: Vec<_> = args.iter().map(|v| ctx.val(*v)).collect();
            ctx.builder.ins().return_call(fref, &cl_args);
        }
        Terminator::TailCallIndirect { fn_ptr, args } => {
            let sig = ctx.indirect_sig.clone();
            let cl_args: Vec<_> = args.iter().map(|v| ctx.val(*v)).collect();
            ctx.builder.ins().return_call_indirect(sig, ctx.val(*fn_ptr), &cl_args);
        }
        Terminator::Trap { code } => {
            ctx.builder.ins().trap(trap_hint_to_cl(code));
        }
    }
}
```

#### `lower/func.rs` — estrutura de um `MirFunc` → Cranelift function

```rust
pub fn lower_func(mir: &MirFunc, ctx: &mut ModuleCtx) {
    let sig = build_signature(mir, ctx);
    let func_id = ctx.module.declare_function(&mir.name, Linkage::Local, &sig).unwrap();

    let mut cl_ctx = ctx.module.make_context();
    cl_ctx.func.signature = sig;

    let mut fb_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut cl_ctx.func, &mut fb_ctx);

    // 1. Criar todos os blocos Cranelift antecipadamente
    let cl_blocks: Vec<Block> = mir.blocks.iter()
        .map(|_| builder.create_block())
        .collect();

    // 2. Adicionar parâmetros ao bloco de entrada (= params da função)
    builder.append_block_params_for_function_params(cl_blocks[0]);

    // 3. Adicionar block params para os demais blocos (phis SSA)
    for (i, bb) in mir.blocks.iter().enumerate().skip(1) {
        for (_, hir_ty) in &bb.params {
            let cl_ty = hir_ty_to_cl(hir_ty, ctx.ptr_ty);
            builder.append_block_param(cl_blocks[i], cl_ty);
        }
    }

    // 4. Lower cada bloco
    let mut fn_ctx = FnCtx::new(builder, cl_blocks, ctx);
    for (i, bb) in mir.blocks.iter().enumerate() {
        fn_ctx.builder.switch_to_block(fn_ctx.cl_blocks[i]);
        // Seal imediatamente se não há back-edges (blocos de loop selados após o back-edge)
        if !is_loop_header(mir, bb.id) {
            fn_ctx.builder.seal_block(fn_ctx.cl_blocks[i]);
        }
        for inst in &bb.insts {
            lower_inst(inst, &mut fn_ctx);
        }
        lower_term(&bb.term, &mut fn_ctx);
    }
    // Selar headers de loop após back-edges terem sido emitidos
    for bb in &mir.blocks {
        if is_loop_header(mir, bb.id) {
            fn_ctx.builder.seal_block(fn_ctx.cl_blocks[bb.id as usize]);
        }
    }

    fn_ctx.builder.finalize();
    ctx.module.define_function(func_id, &mut cl_ctx).unwrap();

    // 5. Extrair stack maps para GC
    if let Some(compiled) = cl_ctx.compiled_code() {
        register_stack_maps(func_id, compiled, ctx);
    }
    ctx.module.clear_context(&mut cl_ctx);
}
```

---

### `rts-runtime`

Todos os namespaces — compilado independente do compilador.

**Mover de:** `src/namespaces/`

```
rts-runtime/src/
  lib.rs
  rt_all.rs         — register_all_symbols(builder: &mut JITBuilder)
  namespaces/       — estrutura atual mantida
```

`rt_all.rs` exporta:

```rust
/// Registra todos os símbolos extern "C" no JITBuilder.
/// Chamado uma vez no startup do módulo JIT.
pub fn register_all_symbols(builder: &mut cranelift_jit::JITBuilder) {
    use crate::namespaces::*;
    builder.symbol("__RTS_FN_NS_IO_PRINT",        io::print as *const u8);
    builder.symbol("__RTS_FN_NS_GC_STRING_NEW",   gc::string_new as *const u8);
    // ... todos os ~400+ símbolos
}
```

`rts-runtime` **não importa** `cranelift-codegen`. A dep de `cranelift_jit::JITBuilder`
é aceitável aqui — é apenas para registro de ponteiros, não para emissão de IR.

---

## Pipeline Completo Pós-Refator

```
Source TS
    │
    ▼  rts-parser
       SWC parse → rts-ast
       Passes AST: expand_async, purity_pass, reduce_pass, array_methods_pass
    │
    ▼  rts-hir
       lower(ast) → HIR com HirType em cada nó
       refine_type_from_init → narrow types propagados (I8..I128, F32, F64)
       numeric_promotion → regras de promoção explícitas
    │
    ▼  rts-mir
       lower(hir) → MIR SSA (BasicBlock + block params + Inst + Terminator)
       passes:
         fold.rs   → strength reduction (mul→shl, div→shr, mod→and) + constant folding
         dce.rs    → eliminação de código morto
         narrow.rs → canonicalização de i8/i16/i32 ops com masks corretas
    │
    ▼  rts-codegen
       abi_bridge.rs → AbiType/LoweredSignature → cranelift ir::Type / ir::Signature
       lower/inst.rs → Inst → builder.ins().*  (1:1 mecânico)
       lower/term.rs → Terminator → jump/brif/return_call/Switch
       lower/func.rs → MirFunc → FunctionBuilder completo
       lower/gc.rs   → declare_value_needs_stack_map + stack map registry
       JITModule (rts run) ou ObjectModule (rts compile)
    │
    ▼
    Binário nativo / memória executável
```

---

## Mapeamento Completo: HirType → Cranelift

| `HirType` | `cranelift ir::Type` | Instrução aritmética | Loads/Stores |
|---|---|---|---|
| `I8`    | `types::I8`   | `iadd`, `isub`, `imul` + mask | `uload8`/`istore8` |
| `I16`   | `types::I16`  | `iadd`, `isub`, `imul` + mask | `uload16`/`istore16` |
| `I32`   | `types::I32`  | `iadd`, `isub`, `imul`, `sdiv` | `sload32`/`istore32` |
| `I64`   | `types::I64`  | `iadd`, `isub`, `imul`, `sdiv` | `load`/`store` |
| `I128`  | `types::I128` | `iadd`, `isub`, `imul` | `load`/`store` (16 bytes) |
| `U8`    | `types::I8`   | `iadd`, `udiv`, `urem` + `band_imm 0xFF` | `uload8`/`istore8` |
| `U16`   | `types::I16`  | `iadd`, `udiv`, `urem` + mask | `uload16`/`istore16` |
| `U32`   | `types::I32`  | `iadd`, `udiv`, `urem` | `uload32`/`istore32` |
| `U64`   | `types::I64`  | `iadd`, `udiv`, `urem` | `load`/`store` |
| `F32`   | `types::F32`  | `fadd`, `fsub`, `fmul`, `fdiv` | `load(F32)`/`store` |
| `F64`   | `types::F64`  | `fadd`, `fsub`, `fmul`, `fdiv` | `load(F64)`/`store` |
| `Number`| `types::F64`  | `fadd`, `fsub`, `fmul`, `fdiv` | `load(F64)`/`store` |
| `Bool`  | `types::I64`  | `icmp` → `uextend I64` | — |
| `Handle`| `types::I64`  | — (opaco) | `load(I64)`/`store` |
| `Str`   | `[I64, I64]`  | — (2 slots: ptr+len) | — |

### Regras de conversão explícita obrigatória no MIR

```
I8  → I64:  sextend(I64, val)   ou  uextend(I64, val) conforme semântica
I64 → I8:   ireduce(I8, val)    ou  band_imm(val, 0xFF) + ireduce
I32 → I64:  sextend(I64, val)
F64 → I64:  fcvt_to_sint_sat(I64, val)   (saturante — não trapa)
I64 → F64:  fcvt_from_sint(F64, val)
F32 → F64:  fpromote(F64, val)
F64 → F32:  fdemote(F32, val)
i64 bits → f64:  bitcast(F64, MemFlags::new(), val)   (transmute)
```

---

## Plano de Execução em Fases

### Fase 0 — Preparação (1–2 dias)

- [ ] Criar `Cargo.toml` de workspace na raiz
- [ ] Criar estrutura de diretórios `crates/`
- [ ] Mover `rts-diagnostics` (zero deps, movimento puro)
- [ ] Mover `rts-ast` — adicionar `TsPrimitive` com narrow types
- [ ] Build verde com monólito ainda em `src/` + novos crates vazios
- [ ] CI rodando workspace (`cargo test --workspace`)

### Fase 1 — Extrações sem mudança funcional (3–5 dias)

- [ ] `rts-abi` — desacoplar `LoweredSignature` de `cranelift-codegen`; implementar `AbiType` próprio
- [ ] `rts-parser` — mover `src/parser/` + passes AST
- [ ] `rts-linker` — mover `src/linker/` + `runtime_objects.rs`
- [ ] `rts-runtime` — mover `src/namespaces/` + exportar `register_all_symbols`
- [ ] `rts-codegen` — mover `src/codegen/` + criar `abi_bridge.rs`; `setup.rs` com `use_egraphs=true`
- [ ] `rts-cli` — mover restante

**Marco:** `cargo test --workspace` verde, mesmos benchmarks.

### Fase 2 — HIR (1–2 semanas)

- [x] Definir `HirType` completo (I8–I128, F32, F64, todos os tipos) em `rts-hir/ir.rs` (commit c2eb724)
- [x] Implementar `lower.rs`: AST → HIR com `HirType` em cada nó (commit c2eb724)
- [x] Implementar `type_refine.rs` + `numeric_promotion` (commit c2eb724)
- [x] Implementar `type_map.rs` com `CraneliftTypeHint` (commit c2eb724)
- [x] Adaptar `rts-codegen` para consumir HIR — primeira fatia: dep + `lower_func` no `compile_program` descartando resultado (commit 2320e1f)
- [x] Testes unitários do crate `rts-hir` (commit c1a22b2, 21/21)
- [ ] Refinar `HirFunc` ret type a partir do body (return stmts com literais inteiros → I64), não só da anotação TS textual
- [ ] Codegen consumir hints HIR: `Call` de user fn em local sem anotação usa `hir_return_types[&fn]` em vez de cair em `Number`
- [ ] Validar: hot loops com `let i = step(i)` emitem `iadd` direto sem `fcvt`

**Marco intermediário (atual):** crate rts-hir completo, conectado ao pipeline, suite verde.
**Marco final:** suite mantida + `rts ir` mostra ausência de `fcvt` em sites tipo `i = userFn(i)` quando a fn retorna inteiro.

### Fase 3 — MIR e passes (2–3 semanas)

- [x] Crate `rts-mir` criado com IR completo: `MirFunc`, `BasicBlock`, `Inst` (60+ variantes incluindo aritmética inteira/float, bitwise, shifts, conversões, comparações, loads/stores narrow, atomics, GC), `Terminator` (Return/Jump/Brif/Switch/TailCall/Trap), `IntCond`/`FloatCond` espelhando Cranelift. 7 testes verde.
- [x] `lower.rs`: HIR → MIR — cobertura ampliada (literais, aritmética, comparações, bitwise, shifts, casts, if/else, while, do-while, for clássico com init/cond/update, break/continue com loop stack, ternary→Select, let/const, return; constructs ainda unsupported caem em `Terminator::Trap`)
- [x] `passes/fold.rs`: constant folding (IAdd/ISub/IMul/SDiv/SRem/BAnd/BOr/BXor de IConst→IConst) + strength reduction (mul→shl, urem→band, sdiv→sshr, ops com const→*Imm)
- [x] `passes/dce.rs`: eliminação de código morto com fixed-point (preserva side-effecting: Store, CallExtern, AtomicStore/Rmw/Cas, Fence, DeclareGcValue)
- [x] `passes/narrow.rs`: canonicalização I8/U8 (mask 0xFF) e I16/U16 (mask 0xFFFF) após IAdd/ISub/IMul/INeg/IShl
- [x] `passes/verify.rs`: invariantes (block ids match position, ValueIds em range, BlockIds em range, params count consistente)
- [x] `crates/rts-codegen/src/mir_codegen/`: lower MIR → Cranelift IR isolado (não plugado no pipeline). hint_bridge.rs converte CraneliftTypeHint → cl::Type; lower.rs traduz Inst/Terminator 1:1 via FunctionBuilder. 8 testes JIT-and-execute verificam que MirFunc construído na mão (e via rts_mir::lower) produz código nativo com retorno correto (iconst, iadd, iaddimm, ishlimm, fadd, brif/icmp, select).
- [ ] `verify.rs`: invariantes em debug (tipo de cada ValueId consistente, blocos terminados)
- [ ] Adaptar `rts-codegen` para consumir MIR via `lower/inst.rs` e `lower/term.rs`
- [ ] Testes unitários para cada pass (sem Cranelift — testam só o MIR)

**Marco:** `cargo test --workspace` verde. Passes verificados com unit tests.

### Fase 4 — Baixo nível e extensões (ongoing)

- [ ] Suporte de primeira classe a `i8`/`i16`/`i32` em TS (sintaxe `let x: i32 = 0`)
- [ ] Operações bitwise e narrow no codegen: `uload8`, `istore16`, `sload32`
- [ ] Atomics expostos via namespace `atomic` já existente + tipos corretos no MIR
- [ ] Inlining de fns pequenas (< N Insts) no MIR pass
- [ ] Escape analysis: handles que não escapem → stack slot via `StackAlloc`
- [ ] SIMD via `HirType::V128(VecKind)` + `Inst::Splat`, `ExtractLane`, `InsertLane`

---

## O Que o Cranelift NÃO Faz (responsabilidade do MIR)

| Otimização | Cranelift faz? | Onde fazer no RTS |
|---|---|---|
| Constant folding interprocessual | Não (só intraproc com e-graphs) | `rts-mir/passes/fold.rs` |
| Inlining | Não | `rts-mir/passes/inline.rs` |
| Escape analysis | Não | `rts-mir/passes/escape.rs` |
| Strength reduction (mul→shl) | Parcial (e-graphs) | `rts-mir/passes/fold.rs` — garante antes do CL |
| Narrow type canonicalização | Não | `rts-mir/passes/narrow.rs` |
| Autovectorização | Não | SIMD deve ser explícito no HIR/MIR |
| DCE interprocessual | Não | `rts-mir/passes/dce.rs` |
| Loop invariant code motion | Limitado | MIR Fase 4 |

O que o Cranelift **faz bem** e não precisamos reimplementar:
- Register allocation (linear scan)
- Instruction selection (escolhe formas nativas por alvo)
- Constant folding intraprocessual (e-graphs com `use_egraphs=true`)
- CSE intraprocessual
- Peephole: `x + 0 → x`, `x * 1 → x`, `x & 0 → 0`, `x | ~0 → ~0`
- Seleção de `br_table` vs binary search (via `Switch` builder)
- Tail call via `return_call` (quando `preserve_frame_pointers=true`)

---

## Diferenças RTS vs Perry — decisões conscientes

| Perry | RTS (pós-refator) | Razão |
|---|---|---|
| NaN-boxing (f64 uniforme) | ABI tipada com `AbiType` | RTS é otimizado para TS bem-tipado; tipos nativos em registradores |
| LLVM backend | Cranelift (mantém) | build leve, licenciamento simples, embutido |
| 30+ crates | ~10 crates | overhead de manutenção menor |
| HIR com monomorfização | HIR sem monomorph (por ora) | generics TS não são tão agressivos |
| MIR implícito no LLVM | MIR explícito leve | Cranelift não faz constant folding/DCE/inlining |
| `perry-jsruntime` (fallback) | sem fallback | RTS compila tudo AOT/JIT |
| Narrow types via NaN-box | Narrow types via `ireduce`/`uextend`/`sextend` explícitos | Mantém ABI tipada, semântica correta |

---

## O Que NÃO Fazer

- **Não migrar para NaN-boxing.** Refator gigante; ganho só em código `any`-pesado;
  ABI tipada é vantagem competitiva do RTS.
- **Não trocar Cranelift por LLVM.** Cranelift cobre tudo via `use_egraphs=true`;
  o gap (autovec, cross-IPO) é coberto pelo MIR layer.
- **Não reimplementar o que o Cranelift já faz** (`x*1`, `x+0`, peephole básico) —
  `use_egraphs=true` já cobre isso intraprocessualmente.
- **Não criar HIR com monomorfização agressiva.** Generics TS simples não justificam.
- **Não separar cada namespace em crate.** Perry fez isso para swap de backend por
  plataforma — RTS não tem esse requisito ainda.
- **Não usar `I128` em hot paths.** A maioria dos backends decompõe em 2 × I64 —
  usar `umulhi`/`smulhi` + `imul` quando precisar de produto 128-bit.

---

## Impacto nos Benchmarks Esperado

| Otimização habilitada | Benchmark afetado | Ganho estimado |
|---|---|---|
| `refine_type_from_init` (HIR) + `iadd_imm` | hot loops com counters não-anotados | 5–15% |
| Strength reduction MIR (mul→shl, mod→and) | qualquer aritmética com constantes | 2–8% |
| Constant folding MIR interprocessual | pipelines com constantes propagadas | variável |
| DCE MIR | código gerado menor → JIT mais rápido | tempo de compile |
| Narrow types corretos (`uload8` etc.) | código que trabalha com bytes/buffers | correto + ~5% |
| Inlining MIR (Fase 4) | fns pequenas em hot loop | 10–30% potencial |

Monte Carlo já está em 16.9 ms AOT — ganho esperado pequeno ali.
Maior ganho: código TS natural sem anotações + código de baixo nível (buffers, FFI).

---

## Referências

- `docs/specs/cranelift-explications.md` — guia completo da API Cranelift 0.131
- `PERRY_ANALYSIS.md` — análise comparativa PerryTS vs RTS
- `docs/specs/silent-parallelism.md` — passes AST a migrar para `rts-parser`
- `docs/specs/async-promise-function.md` — subsistema async (permanece em `rts-runtime`)
- Issues: #90 (block params), #96 (otimizações pendentes), #97 (function pointers)
- Cranelift `use_egraphs` design: https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/docs/egraphs.md
