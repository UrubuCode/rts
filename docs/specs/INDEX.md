# Especificacoes e Notas Tecnicas

Indice de documentos de design, especificacoes de features e decisoes
arquiteturais. Para direcao de alto nivel do projeto consulte
`../../NEXT_STEPS.md` e `../../ROAD_MAP.md` na raiz do repositorio.

## Guias ativos

- [Como criar um namespace](namespace-creation-guide.md) — Processo atual
  baseado em `src/abi/` (SPECS centralizado, simbolos `__RTS_FN_NS_*`,
  `AbiType`). Reflete a branch `feat/remake-namespaces`.
- [Silent parallelism (Level-1)](silent-parallelism.md) — Como o codegen
  detecta padroes `for...of`, reduces, e `arr.map/forEach/reduce` e
  reescreve transparentemente para `parallel.*`. Pipeline dos 3 passes,
  criterio de pureza, infra de suporte (HandleTable shard-aware,
  callconv), limitacoes.
- [async / Promise / Function](async-promise-function.md) — Subsistema
  unificado de async/await, Promise<T>, e Function class. Pipeline do
  desugar `expand_async_functions` (`async fn` → `promise.create`),
  trampolim invoke_n, `new Function` via eval em runtime, integracao
  Promise+Function (`resolve_callback_ptr`). PRs #428-#437, design
  Promise-centric @drysius.

## Subsistemas recentes (2026-05)

Documentacao primaria fica nos doc-comments dos arquivos Rust. Resumo:

- **HTTP server nativo** — `src/namespaces/http_server/`. Servidor
  HTTP/1.1 via actix-web. Bridge sync→async com shard map de slots +
  oneshot channel. Pico medido 29k req/s (78% do actix puro). API:
  `serve(addr, handler)`, `req_method/path/body`, `respond`. PR #400.
- **Runtime tokio compartilhado** — `src/runtime/async_rt.rs` +
  `src/runtime/tokio_ctx.rs`. `OnceLock<Runtime>` global com hooks
  `on_thread_start/stop` que registram workers no `gc/thread_registry`.
  `tokio_ctx` oferece "id u64 opaco + shard map por TypeId" generico
  para qualquer feature async. PR #401, issue #399.
- **GC mark+sweep com stack maps Cranelift** — `src/codegen/jit.rs` +
  `src/namespaces/gc/collector.rs`. Stack scanner usa
  `GetCurrentThreadStackLimits` (Win32 oficial — `gs:[0x10]` retornava
  StackBase < RSP em alguns contextos, bug fix em PR #400). Scan
  multi-thread via `SuspendThread + GetThreadContext` + registers
  callee-saved. Issue #397.
- **4 tipos de spawn coexistindo** — `src/namespaces/thread/abi.rs` tem
  tabela comparativa: `spawn` (std::thread, 30k/s), `spawn_async`
  (tokio, 400k/s), `spawn_async_join` + `join_async` (tokio com
  retorno, 400k/s), `spawn_detached` (pool fixo, 5M/s mas queue
  ilimitada). PRs #401-#403.

## Historico / pendente de reescrita

Os documentos abaixo descrevem versoes anteriores do runtime e ainda nao
foram reescritos para o novo contrato ABI. Use-os apenas como referencia
historica; nao os tome como guia para novo codigo.

- [app-features.md](app-features.md) — Roadmap de features do runtime. Muitos
  itens foram reorganizados; alinhar com `ROAD_MAP.md` antes de consultar.
- [perf-hot-path-optimization.md](perf-hot-path-optimization.md) — Notas da
  otimizacao do hot path (`rts_simple.ts`) antes da remocao de
  `__rts_call_dispatch`. Os numeros permanecem validos como marcador
  historico, mas o caminho descrito nao e mais o atual.
- [rtslib-external-namespaces.md](rtslib-external-namespaces.md) — Design de
  pacotes `.rtslib` externos. Depende da nova ABI estabilizar antes de ser
  retomado.

## Pendencias conhecidas

Itens acompanhados em `NEXT_STEPS.md` / `ROAD_MAP.md`:

- **gc-arena ainda nao integrado** — issue #393. Apesar do
  `gc-arena = "0.5"` no Cargo.toml e referencias em comentarios, a
  `HandleTable` atual e' slotmap+Mutex sharded com mark+sweep proprio
  via stack maps Cranelift. `collect_debt`/`finish_cycle` redirecionam
  para esse mark+sweep, nao para gc-arena. Migracao real exige refator
  grande (todas as 25+ variantes de `Entry` precisam derivar `Collect`
  + `Mutation<'gc>` token cruzando JIT, incompativel com ABI atual).
- Semantica de modulos top-level.
- Pipeline sem stubs de funcao.
- Link fallback multi-objeto.
- Promises sem vazamento (Promise atual e' sincrona resolvida na hora
  — pre-async/await real).
- async/await real no codegen (continuation-passing transform). Sem
  isso a Promise atual quebra na primeira vez que codigo TS faz `await`
  de algo que precisa pausar de verdade.
