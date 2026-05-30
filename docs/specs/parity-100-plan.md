# Plano de ataque — paridade cross-runtime 100% (#226)

> Mapeamento gerado por triagem multi-agente dos 52 testes que faltavam
> (estado: 320/372 = 86.0%). Cada teste foi rodado e diagnosticado; clusters
> agrupados por causa-raiz compartilhada e priorizados por ROI
> (testes-destravados / esforco). Atualizar o checklist conforme os clusters
> fecham. Fonte: workflow `parity-100-triage`.
>
> ## Progresso (sessao 2026-05-30)
>
> Estado real atual: **324 pass** (regenerado via cross_runtime_check.sh; o
> report committed estava stale — varios testes ja' passavam por correcao
> colateral, ex: 193_reflect_construct, 98_proxy_invariants).
>
> Fechados nesta sessao:
> - **[x] H / 98_proxy_invariants** (PR #1306) — Object.* proxy-aware +
>   JSON.stringify(proxy) via traps ownKeys/getOwnPropertyDescriptor/get.
> - **[x] 69_atomics_sharedarraybuffer** (PR #1308) — Atomics.* (RMW/CAS/
>   load/store) + SharedArrayBuffer typed-array views + typeof.
> - **[x] 345_string_template_tag** (PR #1309) — inferencia de retorno
>   string p/ `reduce_bound` com init string (tag fn com capture).
> - **[x] 349_object_descriptors** (PR #1310) — fix regressao do #98:
>   preserva `configurable` em getOwnPropertyDescriptor.
> - **[~] 68_arraybuffer_transfer_clone** (PR #1311, PARCIAL) — view.set(arr)
>   Buffer-aware p/ views NOMEADAS. Falta: views ANONIMAS
>   (`new Uint8Array(buf)` sem binding) + structuredClone transfer/detach.
>
> Reclassificacoes apos investigacao (NAO eram quick wins como o plano supos):
> - **76_message_channel**: precisa runtime MessageChannel completo (port1/2,
>   onmessage, postMessage) + microtask delivery — fundacao, NAO fallback
>   `obj.prop.method()`.
> - **Cluster A (348/361/386/360)**: closures-de-closures + mutable capture
>   (`acc` capturado, `() => f(n-1, acc)`) — eh #195 mutable closures
>   (bloqueado por #90), fundacao GRANDE. 386_trampoline confirma: retorna
>   closure que captura param mutavel.
> - **68**: stack de 4 bugs (named view set [feito], anonymous view read/set,
>   structuredClone buffer-byte-copy, detach) — nao era PEQUENO-MEDIO.
>
> Tambem fechados:
> - **[x] 275_generator_yieldstar** (PR #1312) — `const r = yield* gen()`:
>   yield* em init de VarDecl capturando ret_value. transform_stmt agora
>   devolve Vec<Stmt> (inline, preserva escopo) + GENERATOR_GET_RET.
> - **[x] 311_console_table + 312_console_dir** (PR #1313) — console.*
>   override runtime (CONSOLE_OVERRIDES side-table). Callbacks aridade fixa.
>
> ## Estado final da sessao: **329 pass / 372 = 88.4%** (era 320/86.0%)
>
> Os 8 rts_diverge + 35 rts_error restantes sao TODOS fundacao (investigado
> caso a caso nesta sessao — nao ha mais quick wins nem fixes cirurgicos):
>
> | Teste(s) | Fundacao necessaria | Issue |
> |---|---|---|
> | 274 symbol_toprimitive | Symbol-keyed function storage + coercao toPrimitive/valueOf/toString c/ hint | #216 + #304 |
> | 299 arguments_object, 272, 305 | `arr[Symbol.iterator]` -> function handle real (protocolo iteravel) | #216/#222 |
> | 271 computed_class_members | Symbol como chave computada em class body | #216 |
> | 310 console_group | rest-param (`...args`) via INVOKE_AUTO — falta flag variadic no FunctionData + packing do overflow | follow-up #1313 |
> | 41 closures_deep, 348/354/359/360/361/365/386 | mutable closures / closures-de-closures | #195 (bloq #90) |
> | 393 promise_microtask, 365, 109, 392, 303 | event loop / microtask ordenada / async iteration | #207 |
> | 68 arraybuffer_transfer | views ANONIMAS (`new Uint8Array(buf)` sem binding) + structuredClone transfer/detach | — |
> | 291 json_bigint, 65 | BigInt type marker | #219 |
> | 275*/276/329/344/368/379 | generator state machine (suspensao real, infinito, .return/.throw+finally) | #211/#477 |
> | 59/71/78 intl_* | Intl runtime completo (locale data) | #225 |
> | 64/86/88/96/101/74/75/85 | Web Streams + Blob/File runtime WHATWG | — |
> | 100 dynamic_import | module namespace real | #223 |
> | 79 subtle_digest | crypto.subtle | — |
> | 354/359 obf_proxy/getter | Proxy fase-2 + closures | #218/#195 |
>
> Recomendacao p/ proxima sessao: escolher UMA fundacao e dedicar foco —
> melhor ROI sao **#216 Symbol-as-key** (destrava 271/272/274/299/305, ~5
> testes) ou **anonymous typed-array views** (68 + typed-buffers). Ambas sao
> refator medio-grande, nao cabem num tick de loop curto.
>
> ## #216 Symbol-as-key — CAMADA 1 entregue (PR #1314)
>
> `obj[symbolKey]`/`obj[varKey]` agora preserva o TIPO REAL do valor (era
> coerce eager p/ string via TPL_COERCE_VEC_SLOT). typeof correto, funcao
> armazenada sob symbol key eh chamavel, number mantem identidade. Isso eh
> pre-requisito das camadas 2/3 abaixo.
>
> ## #216 CAMADA 2 entregue (PR #1315 + #1316)
> - **[x] 274_symbol_toprimitive** (PR #1315) — dispatch de
>   `[Symbol.toPrimitive](hint)` nos 3 sites de coercao (+obj/String/binario)
>   via __RTS_FN_RT_TO_PRIMITIVE. 271_computed_class_members tambem fechou
>   colateralmente.
> - **[x] 94_sparse_array_enumeration** (PR #1316) — fix da regressao que a
>   camada 1 (#1314) expos: String(arr[k]==0) dava "null". Agora NUM_BIAS.
>
> Estado: **329/372 = 88.4%**. Restam 6 diverge estaveis (291 bigint, 299
> symbol.iterator, 310 console rest-param, 393 promise microtask, 41 closures,
> 68 anonymous views) + 116 (FLAKE do event loop, passa 3/3 re-rodado).
>
> ## #216 CAMADA 3 entregue (PR #1317) — **331/372 = 89.0%**
> - **[x] 299_arguments_object** (PR #1317) — `arr[Symbol.iterator]` ->
>   handle Function nativo (ARRAY_VALUES_ITER via MAP_GET_KH well-known).
>   typeof "function" + chamavel. 271 tambem confirmado fechado.
>
> Restam 5 diverge estaveis, TODOS fundacao pesada:
> - **291_json_bigint** — BigInt type marker (#219).
> - **310_console_group** — rest-param (`...args`) via INVOKE_AUTO: falta
>   flag variadic no FunctionData (follow-up #1313).
> - **393_promise_microtask** — event loop / microtask ordenada (#207).
> - **41_closures_deep** — mutable closures (#195, bloq #90).
> - **68_arraybuffer_transfer_clone** — views ANONIMAS + structuredClone
>   transfer/detach.
>
> 272/305 (iterator protocol manual / iterator helpers) seguem rts_error —
> precisam de `it.next()` em loop sobre iterator user (ja' segfaltava na
> main; classe #211/#477 state machine). Fora do alcance de fix cirurgico.
>
> Caminho exato p/ FECHAR o restante de #216 (299/272/305):
> - **274_symbol_toprimitive [FEITO]**: operadores de coercao detectam e
>   invocam `[Symbol.toPrimitive](hint)`:
>   - `+obj` (unario) -> hint "number"; `String(obj)` -> hint "string";
>     `obj + ""` (binario, default) -> hint "default".
>   - Sites: lower_typeof/unary em basics.rs (`+`), coerce.rs (String()),
>     operators.rs (binario `+`). Cada um, quando o operando eh um Map
>     handle, deve checar se tem key `@@sym:<toPrimitive_handle>` e invocar
>     via INVOKE_AUTO passando o hint string. Fallback p/ valueOf/toString
>     (#304). NB: o method-shorthand `[Symbol.toPrimitive](h){}` JA' eh
>     armazenado e chamavel (verificado: `m("number")` retorna 7) — falta
>     so' o dispatch na coercao.
> - **299_arguments_object / 272 / 305**: `arr[Symbol.iterator]` deve
>   resolver p/ um function handle real (built-in array iterator). Hoje
>   retorna 0 (key ausente no Vec) -> typeof "number". Precisa: quando o
>   computed-read tem key == Symbol.iterator E o obj eh Vec/array-like,
>   sintetizar/retornar um handle Function de iterator (cursor sobre o Vec,
>   ja' existe ITERATOR_FROM em gc/generator.rs). typeof desse handle ->
>   "function".
> - **271_computed_class_members**: Symbol como chave computada em CLASS
>   body (nao object literal) — desugar_object_methods nao cobre class;
>   precisa do equivalente em compile/class.rs.

# PLANO DE ATAQUE — RTS rumo a 100% paridade cross-runtime (52 testes)

Validei as causas-raiz no código: `generator_desugar.rs` (transform_stmt sem Decl/Try, catch-all linha 138), `captures.rs` (`collect_captured_from_arrow` só trata `Pat::Ident`), `hoist_fn.rs` (declara explicitamente que NÃO resolve capturas), `this_arrow.rs` (naming `__lifted_arrow_N` sem prefixo de classe), `GLOBAL_CLASS_SPECS` em `abi/mod.rs`, e `lower_console_call` hardcoded. Os agrupamentos abaixo refletem isso.

---

## (1) CLUSTERS POR CAUSA-RAIZ COMPARTILHADA

### Cluster A — Captura de closures em arrow/fn liftadas (FUNDAÇÃO QUENTE)
**Causa única:** os passes de lift/hoist (`passes/this_arrow.rs`, `passes/hoist_fn.rs`, `analysis/captures.rs`) não fazem análise de captura de variáveis livres nem desempacotam rest/destructuring params. Variáveis do escopo pai viram "undefined variable" e o naming não preserva contexto de classe.
**Testes (11):** 348_closure_optimization, 354_obf_proxy_trap, 359_obf_getter_smuggle, 360_obf_iife_scope, 361_functional_compose, 363_algorithms, 365_async_patterns, 376_private_pattern_advanced, 386_trampoline, 41_closures_deep, 378_symbol_species_hasinstance (parcial — arrow callback em método de classe).
**Tamanho:** GRANDE mas único. Subdividível: (A1) capture analysis básica de free vars; (A2) rest/destructuring params em arrow liftada; (A3) prefixo `__class_C_` quando `in_class`. A3 sozinho destrava 376.

### Cluster B — Generator desugar incompleto (yield em posições/statements não cobertos)
**Causa única:** `transform_stmt()` não tem case para `Stmt::Decl` (VarDecl com yield no init) nem `Stmt::Try`; catch-all linha 138 deixa `Expr::Yield` cru → codegen rejeita.
**Testes (7):** 275_generator_yieldstar, 276_generator_return_throw, 344_bundler_helpers, 368_iterator_protocol, 379_yield_star, 273_symbol_asynciterator (gerador em object method), 329_generator_basics (parcial — também precisa suspensão real).
**Tamanho:** MÉDIO para B1 (adicionar Decl+Try ao transform_stmt + recursar em transform_expr para yield em init). GRANDE para 329 (suspensão real vs eager buffering — fundação separada, ver cluster J).

### Cluster C — Web Streams + Blob/File (GlobalClassSpec ausentes)
**Causa única:** classes não registradas em `GLOBAL_CLASS_SPECS`, sem implementação runtime.
**Testes (7):** 64_readable_stream, 86_blob_stream, 88_compression_stream, 96_streams_transform, 101_textdecoder_stream, 74_blob_basics, 75_file_basics, 85_request_response (Response/Request constructor).
**Tamanho:** GRANDE (precisa runtime real de streams WHATWG). Blob/File (74/75) são MENORES e fundacionais (File extends Blob; Blob é pré-req dos streams de blob).

### Cluster D — console.* não-substituíveis (hardcoded compile-time)
**Causa única:** `lower_console_call` (builtins.rs:898) despacha por pattern sintático, ignorando reatribuição runtime e member access.
**Testes (3):** 310_console_group, 311_console_table, 312_console_dir.
**Tamanho:** MÉDIO. Precisa tornar métodos console propriedades reais (function handles) no objeto console, permitindo indirect call/override.

### Cluster E — Intl completo (GlobalClassSpec + runtime)
**Causa única:** nenhuma classe Intl em `GLOBAL_CLASS_SPECS`, sem ABI methods.
**Testes (3):** 59_intl_basics, 71_intl_segmenter, 78_intl_more.
**Tamanho:** GRANDE (NumberFormat/DateTimeFormat/Collator/PluralRules/ListFormat/RelativeTimeFormat/Segmenter — locale data pesada).

### Cluster F — Symbol como chave/iterator em runtime
**Causa única:** computed Symbol keys viram string snippets; built-in collections sem Symbol.iterator instalado.
**Testes (5):** 271_computed_class_members, 272_symbol_iterator_custom, 299_arguments_object, 305_iterator_helpers, 274_symbol_toprimitive. (272 também depende de desugar de method shorthand em class body.)
**Tamanho:** GRANDE (side-channel HashMap p/ Symbol keys, protocolo iterável).

### Cluster G — Async event loop / microtask real
**Causa única:** Promise.then usa spawn_blocking concorrente em vez de microtask queue ordenada; for-await-of não implementado.
**Testes (5):** 393_promise_microtask, 365_async_patterns (parcial), 109_async_generator, 392_async_iterator_advanced, 303_array_fromasync.
**Tamanho:** REFATOR GRANDE (issue #207). Async iteration adicional bloqueada por generators.

### Cluster H — Proxy trap dispatch em Object.* (phase 1 não atualizada)
**Causa única:** `Object.defineProperty`/`getOwnPropertyDescriptor` chamam backend direto em vez das versões proxy-aware que já existem em `proxy/ops.rs`.
**Testes (1):** 98_proxy_invariants.
**Tamanho:** PEQUENO (redirecionar 2 call sites para `REFLECT_*_PROXY` existentes).

### Cluster I — BigInt preservação de tipo
**Causa única:** BigInt vira i64 no codegen (basics.rs:80-84), tipo apagado.
**Testes (2):** 291_json_bigint, 65_bigint_typed_arrays (parcial — BigInt64Array). (291 precisa marker de tipo; 65 precisa registrar BigInt64Array em typed_array_kind + DataView methods.)
**Tamanho:** MÉDIO-GRANDE para marker geral; 65 isolado é MÉDIO (registrar tipo + ABI methods).

### Cluster J — Generators com suspensão real (state machine)
**Causa única:** desugar eager para array buffer; sem suspensão.
**Testes (1 isolado + sobreposição):** 329_generator_basics (e a base de #211 que afeta async generators).
**Tamanho:** REFATOR GRANDE (issue #211/#477, state machine).

### Itens isolados (sem cluster grande)
- **374_private_static_methods** [PEQUENO] — Map constructor não consome iterable direto de `arr.map()`. Fix no codegen do `new Map(iterable)`.
- **345_string_template_tag** [PEQUENO] — TemplateStringsArray não registrado em `ctx.local_array_vars`.
- **68_arraybuffer_transfer_clone** [PEQUENO-MÉDIO] — `VEC_SET_FROM` ignora `Entry::Buffer`; adicionar path Buffer-aware.
- **76_message_channel** [PEQUENO] — falta fallback genérico p/ `obj.prop.method()` (receiver Member não-Ident) em calls/mod.rs.
- **79_subtle_digest** [MÉDIO] — crypto.subtle ausente; crypto como string sentinel.
- **69_atomics_sharedarraybuffer** [PEQUENO] — SharedArrayBuffer faltando na whitelist de builtins (expressions/mod.rs:205-209).
- **100_dynamic_import** [GRANDE] — import() vira subprocess; precisa module namespace real (issue #223).
- **394_structuredclone_complex** [MÉDIO] — destructuring de expr composta gera `__destruct_N` não registrado em escopo.

---

## (2) CLUSTERS ORDENADOS POR ROI (destravados / esforço)

| # | Cluster/Item | Testes | Esforço | ROI |
|---|---|---|---|---|
| 1 | **H** — Proxy Object.* → proxy-aware | 1 | XS (2 call sites) | **Altíssimo** |
| 2 | **69** SharedArrayBuffer whitelist | 1 | XS | **Altíssimo** |
| 3 | **345** TemplateStringsArray em local_array_vars | 1 | XS | **Altíssimo** |
| 4 | **76** fallback `obj.prop.method()` | 1 | S | Alto |
| 5 | **374** new Map(iterable direto) | 1 | S | Alto |
| 6 | **B1** — generator desugar Decl+Try | ~5-6 | M | **Alto** |
| 7 | **A** — closure capture-lift (A3 primeiro) | até 11 | M→G | **Alto** (maior payoff total) |
| 8 | **68** VEC_SET_FROM Buffer-aware | 1 | S-M | Médio |
| 9 | **394** destructuring temp var scope | 1 | M | Médio |
| 10 | **D** — console.* substituíveis | 3 | M | Médio |
| 11 | **65** BigInt64Array + DataView | 1 | M | Médio |
| 12 | **79** crypto.subtle | 1 | M | Médio |
| 13 | **C** — Blob/File primeiro, depois Streams | 7 | G | Médio (Blob/File destravam) |
| 14 | **F** — Symbol iterator/keys | 5 | G | Baixo-Médio |
| 15 | **I/291** BigInt marker geral | 1 | M-G | Baixo |
| 16 | **E** — Intl completo | 3 | G | Baixo |
| 17 | **G** — event loop/microtask | 5 | XG | Baixo |
| 18 | **J/329** generator state machine | 1+ | XG | Baixo |
| 19 | **100** dynamic import real | 1 | G | Baixo |

**Quick wins imediatos (itens 1-7):** ~12-18 testes com esforço majoritariamente S/M. Atacar nessa ordem antes de qualquer fundação pesada.

---

## (3) PRIMEIRO PASSO CONCRETO — 3 maiores ROI

### Cluster A (capture-lift, até 11 testes) — MAIOR PAYOFF
Começar por **A3 (naming, destrava 376 sozinho e desbloqueia validação de private scope)**, depois A1.
- **Arquivo:** `crates/rts-codegen/src/codegen/lower/passes/this_arrow.rs`. Já existe lógica `__class_C_lifted_arrow_N` (linha 6, usada quando captura `this`/super em `in_class=true`, ver linhas 444/447/464). Estender para que **toda** arrow liftada dentro de método de classe receba prefixo `__class_<C>_`, de modo que `extract_class_owner("__class_C_lifted_arrow_0")` retorne o nome e `ctx.current_class` fique setado (corrige 376).
- **A1 (capturas):** em `analysis/captures.rs::collect_captured_from_arrow` já há coleta de free vars; o gap é que `hoist_fn.rs` (linhas 7-9 dizem explicitamente "Nao tenta resolver capturas") não a usa. Passo: fazer `hoist_fn` invocar a análise de captura e injetar as vars capturadas como params extra da fn sintética + reescrever call site para passá-las. Ampliar `collect_captured_from_arrow` (captures.rs:227-231) para tratar `Pat::Assign/Array/Object/Rest` (corrige 386).

### Cluster B1 (generators, ~5-6 testes) — confirmado no código
- **Arquivo:** `crates/rts-parser/src/generator_desugar.rs`, função `transform_stmt` (linha ~71-138). Adicionar arms para `Stmt::Decl(Decl::Var(...))` — recursar em cada `VarDeclarator.init` via um `transform_expr` que reescreve `Expr::Yield`/`yield*` em posição de expressão (push para `__gen_buf` + bind do resultado) — e para `Stmt::Try` (transformar body/handler/finalizer recursivamente). Remover a dependência do catch-all linha 138 para esses casos. Destrava 275, 276, 344, 368, 379 e ajuda 273.

### Cluster H (Proxy Object.*, 1 teste, XS) — quick win de abertura
- **Arquivo:** `crates/rts-codegen/src/codegen/lower/expressions/members.rs` (ou calls). Localizar os call sites que emitem `MAP_DEFINE_PROPERTY` (~linha 1507) e `REFLECT_GET_OWN_PROPERTY_DESCRIPTOR` (~linha 1343) e redirecioná-los para `REFLECT_DEFINE_PROPERTY_PROXY` / `REFLECT_GET_OWN_PROPERTY_DESCRIPTOR_PROXY` (já implementados em `crates/rts-runtime/src/namespaces/.../proxy/ops.rs`), espelhando o que apply/construct/get/set já fazem. Corrige 98 com mudança mínima.

---

## (4) FUNDAÇÃO PESADA — deixar por último ou candidate-discard

- **Cluster G (event loop / microtask real, #207):** refator do modelo de execução de Promise — trocar `spawn_blocking` por microtask queue ordenada com interleaving em await points. Bloqueia 393, 392, 109, 365(parcial), 303. **Fundação pesada, por último.** É a única forma de corrigir ordering determinístico; não há fix incremental.
- **Cluster J (generator state machine, #211/#477):** suspensão real exige reescrever desugar de array-buffer para máquina de estados. Bloqueia 329 e todos async generators. **Pesado; depende de #207 para a parte async.**
- **Cluster E (Intl completo, #225):** locale data + 7 classes. Alto custo, só 3 testes. **Candidate-discard ou último.**
- **100_dynamic_import (#223):** module namespace real em vez de subprocess. Isolado e caro. **Candidate-discard.**
- **Cluster F (Symbol keys/iterator, #216/#222):** side-channel HashMap para Symbol keys + protocolo iterável em built-ins. Médio-grande; 5 testes. Fazível mas depois dos quick wins.
- **291 BigInt marker geral (#219):** preservar tipo BigInt através de codegen/runtime é estrutural. 65 (BigInt64Array isolado) é fazível incrementalmente; 291 (validação em JSON.stringify) exige o marker — **deixar 291 para depois**.

---

**Sequência recomendada de execução:** quick wins H→69→345→76→374 (≈5 testes, dias), depois B1 (≈5-6 testes) e Cluster A em fatias A3→A1→A2 (≈11 testes) — isso sozinho leva de 52 faltantes para ~30. Em seguida D (console), 65, 79, 68, 394, e Blob/File do cluster C. As fundações pesadas (G, J, E, F, dynamic import, BigInt marker) ficam para o fim, com G+J sendo os bloqueadores duros de async/generators que provavelmente definem se 100% é alcançável sem refator de runtime.
## Estado pos-sessao 2026-05-30 (continuacao): 332/372 = 89.2%

12 PRs mergeados na sessao (98/69/345/349/68p/275/311/312/274/271/94/299/310 +
fundacao #216 completa). Restam 4 diverge, TODOS fundacao pesada sem fix
cirurgico possivel:

- **291_json_bigint** — BigInt type marker (#219): BigInt vira i64 no codegen,
  tipo apagado; JSON.stringify(bigint) deveria lancar TypeError.
- **393_promise_microtask** — event loop / microtask queue ordenada (#207):
  Promise.then usa spawn_blocking concorrente em vez de fila FIFO determinista.
- **41_closures_deep** — mutable closures (#195, bloqueado por #90): closure
  que captura var mutavel por referencia + closures-de-closures.
- **68_arraybuffer_transfer_clone** — views ANONIMAS (`new Uint8Array(buf)` sem
  binding, ler/escrever) + structuredClone com transfer/detach do buffer.

Os 35 rts_error sao clusters grandes: async generators / for-await (#207),
generator state machine infinito (#211/#477), Intl runtime (#225), Web Streams
+ Blob/File (WHATWG), dynamic import (#223), crypto.subtle, Proxy fase-2.

Nenhum dos 4 diverge nem dos 35 error cabe num tick de loop — cada um exige
refator de fundacao dedicado (event loop real, BigInt como tipo de 1a classe,
env-record p/ mutable closures, anonymous-view codegen, etc).
