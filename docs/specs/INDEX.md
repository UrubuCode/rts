# Especificacoes e Notas Tecnicas

Indice de documentos de design, especificacoes de features e decisoes
arquiteturais. Para direcao de alto nivel do projeto consulte
[`../../RTS_REFACTOR.md`](../../RTS_REFACTOR.md) na raiz — plano canonico
do refator em workspace de 10 crates (`crates/rts-ast`, `rts-abi`,
`rts-codegen` com `mir_codegen/`, `rts-cli`, `rts-diagnostics`,
`rts-hir`, `rts-mir`, `rts-linker`, `rts-parser`, `rts-runtime`). A
Fase 3 do refator esta entregue: o crate `rts-mir` eh a camada nova
SSA entre HIR e Cranelift, ativa por default desde commit f7b924b
(routing hibrido por `RTS_USE_MIR`, fallback automatico para AST em
constructs nao modelados). Fase 4 (baixo nivel + extensoes) em
progresso, 5/8 entregues: atomics (4.1), inline + integracao +
fixed-point (4.2/4.3/4.7), CSE intra-bloco (4.5), FMA fusion (4.8),
arr[i]=v + smoke e2e (4.4/4.6). Restam escape analysis, SIMD e
narrow storage real.

## Guias ativos

- [Cross-runtime parity testing](cross-runtime-testing.md) — Sistema CI
  que valida RTS vs Bun vs Node em fixtures TS standalone. Diff de stdout
  linha-a-linha, issue automatica em divergencia detectada em schedule.
- [Cross-runtime coverage roadmap](cross-runtime-roadmap.md) — Lista
  viva das fixtures planejadas (63 itens prioritarios + sugestoes
  futuras). Checklist marcavel conforme batches sao adicionados pelo
  codex/kiro.
- [Plano de ataque — paridade 100%](parity-100-plan.md) — Triagem
  multi-agente dos 52 testes que faltam (86.0% -> 100%): clusters por
  causa-raiz compartilhada, tabela ROI (destravados/esforco), passos
  concretos. Atacar do maior ROI ao menor; refatores pesados
  (generators, event loop, Intl) por ultimo.
- [Como criar um namespace](namespace-creation-guide.md) — Processo atual
  baseado em `crates/rts-abi/` (SPECS centralizado, simbolos `__RTS_FN_NS_*`,
  `AbiType`). Reflete a branch `feat/remake-namespaces`.
- [Suporte a `.node` (Node native addons)](node-format/README.md) — Estudo
  multi-fonte (8 docs) sobre o formato `.node`, a ABI N-API e as divergencias
  para o RTS dar suporte a addons nativos do npm. Veredito: viavel so via N-API
  (`napi_value`/`napi_env` opacos -> mapeaveis a HandleTable sem V8),
  JIT-first, nunca V8-direto/NAN. Verificado por 2 runs adversariais (115
  afirmacoes, 0 refutadas). Ponto de integracao: `resolve_node_modules_import`.
- [Silent parallelism (Level-1)](silent-parallelism.md) — Como o codegen
  detecta padroes `for...of`, reduces, e `arr.map/forEach/reduce` e
  reescreve transparentemente para `parallel.*`. Pipeline dos 3 passes,
  criterio de pureza, infra de suporte (HandleTable shard-aware,
  callconv), limitacoes.
- [Auto-paralelismo por regioes (motor novo)](auto-parallel-regions.md) —
  Re-justificativa do paralelismo no motor novo por PROVA (nao chute de
  AST): modelo de 3 regioes (thread-local / shared-imutavel /
  shared-mutavel), gate de 2 passes (seguro + vale), trava dura do GC
  (`SuspendThread`+lock => shared-mutavel so via intrinsic atomico
  1-call). Base teorica: escape analysis, effect/region (DPJ),
  commutativity (Rinard), TLS (rejeitado). Inclui explicacao do async
  RMW atomico (motor velho, ref) e secao "por que NAO e util" (limites,
  o que rejeitar). Pre-requisito: pos-P5.
- [Epic #226 — paridade JS/TS](js-parity-epic-226.md) — Lote PRs
  #483-#547: ~60 APIs JS adicionadas (Array/Object/Math/String/Symbol/
  URL/Date/Boolean/parseInt/destructuring), bugs corrigidos no caminho,
  tabela de issues abertas pesadas. Suite: **977/977 (100%)** apos lotes
  recentes (sessao 2026-05-09): module system completo (#213/#618/#619),
  Reflect API + Proxy completo com 13 traps (#218), divisao JS spec (#584),
  hard-fail em ident desconhecido (#383), arguments.length (#450), e mais.
- [Reflect + Proxy](reflect-proxy.md) — API Reflect (13 metodos) + Proxy
  com todas as 13 traps. Design: handle Entry::Proxy { target, handler }
  com hooks em MAP_GET_CHAIN/MAP_SET/INVOKE_AUTO/etc. Forward automatico
  ao target quando trap ausente. Limitacoes documentadas (mutable closure
  em trap, k:any nao reusavel em Reflect.* dentro do trap).
- [async / Promise / Function](async-promise-function.md) — Subsistema
  unificado de async/await, Promise<T>, e Function class. Pipeline do
  desugar `expand_async_functions` (`async fn` → `promise.create`),
  trampolim invoke_n, `new Function` via eval em runtime, integracao
  Promise+Function (`resolve_callback_ptr`). PRs #428-#437, design
  Promise-centric @drysius.

## Subsistemas recentes (2026-05)

Documentacao primaria fica nos doc-comments dos arquivos Rust. Resumo:

- **HTTP server nativo** — `crates/rts-runtime/src/namespaces/http_server/`. Servidor
  HTTP/1.1 via actix-web. Bridge sync→async com shard map de slots +
  oneshot channel. Pico medido 29k req/s (78% do actix puro). API:
  `serve(addr, handler)`, `req_method/path/body`, `respond`. PR #400.
- **Runtime tokio compartilhado** — `crates/rts-runtime/src/runtime/async_rt.rs` +
  `crates/rts-runtime/src/runtime/tokio_ctx.rs`. `OnceLock<Runtime>` global com hooks
  `on_thread_start/stop` que registram workers no `gc/thread_registry`.
  `tokio_ctx` oferece "id u64 opaco + shard map por TypeId" generico
  para qualquer feature async. PR #401, issue #399.
- **GC mark+sweep com stack maps Cranelift** — `crates/rts-codegen/src/codegen/jit.rs` +
  `crates/rts-runtime/src/namespaces/gc/collector.rs`. Stack scanner usa
  `GetCurrentThreadStackLimits` (Win32 oficial — `gs:[0x10]` retornava
  StackBase < RSP em alguns contextos, bug fix em PR #400). Scan
  multi-thread via `SuspendThread + GetThreadContext` + registers
  callee-saved. Issue #397.
- **4 tipos de spawn coexistindo** — `crates/rts-runtime/src/namespaces/thread/abi.rs` tem
  tabela comparativa: `spawn` (std::thread, 30k/s), `spawn_async`
  (tokio, 400k/s), `spawn_async_join` + `join_async` (tokio com
  retorno, 400k/s), `spawn_detached` (pool fixo, 5M/s mas queue
  ilimitada). PRs #401-#403.

## Historico / pendente de reescrita

Os documentos abaixo descrevem versoes anteriores do runtime e ainda nao
foram reescritos para o novo contrato ABI. Use-os apenas como referencia
historica; nao os tome como guia para novo codigo.

- [app-features.md](app-features.md) — Roadmap de features do runtime. Muitos
  itens foram reorganizados; alinhar com `RTS_REFACTOR.md` antes de consultar.
- [perf-hot-path-optimization.md](perf-hot-path-optimization.md) — Notas da
  otimizacao do hot path (`rts_simple.ts`) antes da remocao de
  `__rts_call_dispatch`. Os numeros permanecem validos como marcador
  historico, mas o caminho descrito nao e mais o atual.
- [rtslib-external-namespaces.md](rtslib-external-namespaces.md) — Design de
  pacotes `.rtslib` externos. Depende da nova ABI estabilizar antes de ser
  retomado.

## Pendencias conhecidas

Itens acompanhados em `RTS_REFACTOR.md`:

- Semantica de modulos top-level.
- Pipeline sem stubs de funcao.
- Link fallback multi-objeto.
- Promises sem vazamento (Promise atual e' sincrona resolvida na hora
  — pre-async/await real).
- async/await real no codegen (continuation-passing transform). Sem
  isso a Promise atual quebra na primeira vez que codigo TS faz `await`
  de algo que precisa pausar de verdade.
