# Otimizacoes de codegen + layout de artefatos + docs

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
`NEXT_STEPS.md` e `ROAD_MAP.md` na raiz.

Specs ativos relevantes:
- `docs/specs/namespace-creation-guide.md` — processo atual baseado
  em `src/abi/`
- `docs/specs/silent-parallelism.md` — pipeline dos 3 passes
- `docs/specs/async-promise-function.md` — sistema async/Promise/
  Function unificado (#359 + #437)
