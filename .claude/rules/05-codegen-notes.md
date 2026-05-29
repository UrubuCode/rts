# Otimizacoes de codegen + layout de artefatos + docs

> Nota: paths de codegen abaixo vivem em
> `crates/rts-codegen/src/codegen/lower/` (caminho AST autoritativo) e
> `crates/rts-codegen/src/codegen/mir_codegen/` (camada MIR ativa por
> default). HIR + MIR estao em producao desde Fase 3 do
> `RTS_REFACTOR.md` (commits f7b924b/23dd4b7).

## Camada MIR (`mir_codegen/`) — paralela ao codegen AST

`crates/rts-codegen/src/codegen/mir_codegen/` consome `MirFunc` do
crate `rts-mir` e emite Cranelift IR via `FunctionBuilder` 1:1.
`hint_bridge` converte `CraneliftTypeHint` em `cl::Type`; `lower.rs`
traduz `Inst`/`Terminator`; `extern_resolver_default()` resolve
namespaces RTS via `crate::abi::SPECS`.

Routing hibrido em `compile_user_fn`: cada user fn tenta MIR; bail
silencioso para AST quando bate em construct nao modelado (member em
this/objetos, classes, async/await, address-taken fns, string em
params/ret de user fn). Default ON; `RTS_USE_MIR=0` desliga,
`RTS_USE_MIR=fn1,fn2,...` restringe.

Optimizacoes a nivel MIR. `optimize()` roda na ordem:
**fold → fma → cse → dce**. Mais `inline` em fixed-point (max 4
iteracoes com `optimize` entre passadas) entre lower e optimize no
`try_compile_via_mir`.

- `passes/fold.rs` — constant folding (IAdd/ISub/IMul/SDiv/SRem/BAnd/
  BOr/BXor de IConst→IConst) + strength reduction (mul→shl, urem→band,
  sdiv→sshr, ops com const→`*Imm`)
- `passes/fma.rs` — FMA fusion `a*b+c → Fma`, conservador (so funde
  quando o `FMul` tem 1 use, evitando duplicar trabalho) (etapa 4.8)
- `passes/cse.rs` — Common Subexpression Elimination intra-bloco
  (etapa 4.5)
- `passes/dce.rs` — eliminacao de codigo morto com fixed-point
  (preserva side-effecting: Store, CallExtern, AtomicStore/Rmw/Cas,
  Fence, DeclareGcValue)
- `passes/inline.rs` — inlining de fns pequenas, `INLINE_BUDGET=16`,
  elegibilidade conservadora (sem recursao); rodado em fixed-point
  ate 4 iters via `MIR_CACHE` thread-local + pre-registro de
  signatures HIR (etapas 4.2/4.3/4.7)
- `passes/narrow.rs` — canonicalizacao I8/U8 (mask 0xFF) e I16/U16
  (mask 0xFFFF) apos IAdd/ISub/IMul/INeg/IShl
- `passes/verify.rs` — invariantes (block ids match position,
  ValueIds em range, params count consistente)
- Intrinsic inlining: tag `Intrinsic` na spec do namespace gera Inst
  especializado (Sqrt, FAbs, FMin/FMax, IAbs, IMin/IMax) em vez de
  `CallExtern`; `mir_codegen` baixa direto pra IR Cranelift nativa.
- Atomics no `mir_codegen` (etapa 4.1): `Inst::AtomicLoad`/`AtomicStore`/
  `AtomicRmw`/`AtomicCas`/`Fence` baixam direto pra `atomic_*` do
  Cranelift com mapeamento `MemOrder`/`RmwOp`.

## Otimizacoes de codegen notaveis

- **Intrinsics inline** (`abi::Intrinsic`): `sqrt`, `abs_f64`,
  `min/max_f64`, `abs_i64`, `min/max_i64`, `random_f64` — emitidos
  como IR Cranelift direto em `lower_intrinsic`
- **Tail call optimization**: user functions em `CallConv::Tail`;
  `return f(x)` em posicao de tail emite `return_call` (exige
  `preserve_frame_pointers=true` em x86-64)
- **First-class function pointers** (#97 fase 1): `Expr::Ident`
  resolvendo a user fn materializa `func_addr` como i64; call via
  ident local/param faz `call_indirect` com signature provisoria
  Tail
- **Jump table switch**: quando todos os non-default cases sao
  literais inteiros, usa `cranelift_frontend::Switch` (backend
  decide `br_table` vs binary search)
- **Imm forms**: `x + N` / `x & MASK` / `x << K` emitem
  `iadd_imm` / `band_imm` / `ishl_imm` sem iconst intermediario
- **MemFlags::trusted** em loads/stores de globals e RNG state
- **f64 modulo** via libc `fmod` (antes truncava via i64 perdendo
  a parte fracionaria)
- **Constantes como propriedades** (`math.PI` sem parens) via
  `MemberKind::Constant` + `emit_constant_load`
- **Function class (#359)**: trampolim `invoke_n` despacha por
  aridade ate 8 via transmute pra `extern "C" fn(i64...) -> i64`.
  Reify de user fn ident em handle Function so' em member access
  (chamadas diretas continuam usando `call_indirect` rapido).
- **expand_async_functions (#437)**: pass simplificado pos-refator
  emite `f = (args) => promise.create(__async_inner_f, args)` em
  vez do wrapper sintetico antigo (~110 LOC a menos por async fn).

## Assembly inline (`std::arch::asm!`) — tecnica disponivel

Quando o problema exige controle de ABI/registradores que Rust
seguro nao expressa (chamar `fn_ptr` com aridade dinamica, ler
RSP/registradores, manipular o frame de chamada), **assembly inline
via `std::arch::asm!` e ferramenta legitima e ja usada no projeto** —
nao e ultimo recurso proibido. Casos vivos:

- **`gc/collector.rs`** — `asm!("mov {}, rsp", ...)` captura o stack
  pointer no scanner de raizes.
- **`globals/function/ops.rs::invoke_all_i64`** (#1281) — trampolim
  Win64 que monta args dinamicamente (4 em RCX/RDX/R8/R9, resto na
  stack com shadow space 32 + alinhamento 16 antes do `call`).
  Substituiu um `match` por-aridade com teto de 8 (resultado errado /
  ACCESS_VIOLATION acima do teto) por **aridade N variavel** sem
  limite artificial.

Regras ao usar asm inline:

- **Sempre `#[cfg(...)]` por target** + **fallback portavel**
  (`#[cfg(not(...))]`) — nao quebrar CI/builds noutras plataformas.
- **Listar todos os clobbers** (caller-saved GP + XMM). NB:
  `clobber_abi("win64")` conflita com `out("rax")` explicito — use
  uma forma ou outra.
- **Respeitar a ABI alvo** (Win64: 4 args em registrador + shadow
  space 32 + stack 16-aligned antes do `call`).
- **Documentar a convencao assumida** em doc-comment.
- Vale a regra de zero-regressao: `cargo test --release --lib` +
  `rts.exe test` apos mudar asm.

Use quando a alternativa segura seria um limite artificial ou
impossivel (ler registradores). Para logica comum, prefira Cranelift
IR / Rust.

## Otimizacoes pendentes / backlog

Ver issues abertas #90, #96, #97 (fases 2/3). #92 autovec foi
fechada como inviavel sem loop vectorizer proprio (Cranelift nao
tem um); Bun ganha em Monte Carlo >1B iter por autovec do V8.

## Layout de Artefatos do Usuario

Alvo da Fase 1 do roadmap (em progresso):

```
<project>/
  src/main.ts
  package.json
  tsconfig.json

  node_modules/.rts/
    objs/
      runtime/        — objects completos do builtin (todos os modulos)
      compile/        — objects AOT com slicing (apenas em rts compile)
    modules/          — modulos resolvidos e cacheados (com metadata .ometa)

  release/            — apenas em rts compile
    <project_name>    — .exe / .dll / .so / .node conforme target
```

## Docs e especificacoes

A pasta `docs/specs/` contem especificacoes de features, decisoes
de design e notas tecnicas. Consultar o indice em
`docs/specs/INDEX.md`. Direcao de alto nivel fica em
`RTS_REFACTOR.md` na raiz (plano canonico do refator em workspace de
crates).

Specs ativos relevantes:
- `docs/specs/namespace-creation-guide.md` — processo atual baseado
  em `crates/rts-abi/`
- `docs/specs/silent-parallelism.md` — pipeline dos 3 passes
- `docs/specs/async-promise-function.md` — sistema async/Promise/
  Function unificado (#359 + #437)
