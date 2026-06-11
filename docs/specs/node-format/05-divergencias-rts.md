# 05 — Divergências fundamentais: RTS × `.node`

> O coração do estudo. Cada divergência classificada como **bloqueador
> fundamental** ou **trabalho de engenharia**, ancorada na arquitetura real do
> RTS (Cranelift, ABI `extern "C"` de tipos de máquina, `HandleTable`, GC
> mark+sweep, tokio).

## Resumo executivo das 5 divergências

| # | Divergência | Natureza | Classificação |
|---|---|---|---|
| 1 | Representação de valor (`napi_value` vs bits/handles RTS) | camada de marshalling | 🟡 engenharia média |
| 2 | ABI/loading (`napi_*` exportado + dlopen vs link estático) | volume + loader | 🟠 engenharia alta (volume) |
| 3 | GC/finalizers (`napi_ref` V8 vs mark+sweep) | hooks de root/sweep | 🟡 engenharia média |
| 4 | Event loop (libuv vs tokio) | ponte + shim uv | 🟠 média-alta, cauda longa |
| 5 | JIT/AOT (dlopen runtime vs binário self-contained) | conflito filosófico | 🔴 bloqueador filosófico no AOT |
| — | **(extra)** addons V8-diretos/NAN | emular layout V8 | 🔴 quase-bloqueador técnico → **fora de escopo** |

**Veredito:** nenhuma das 5 é bloqueador **absoluto**. A barreira real é
**volume** + a **cauda longa** de `v8::` cru/`uv_*`, e o **conflito filosófico**
com o `.rtslib` estático no modo AOT.

---

## Divergência 1 — Representação de valor 🟡

**O problema.** Addons N-API operam sobre `napi_value` (handle opaco). O RTS
representa valores JS como **bits nativos** `i64`/`f64` ou **handles `u64`** numa
`HandleTable`. Não há "objeto JS no heap" no sentido V8.

**Por que NÃO é bloqueador.** O addon **nunca** dereferencia `napi_value` — só o
passa de volta às funções `napi_*`. Logo o RTS controla 100% da representação.
O Bun é a prova de existência (mapeia `napi_value` → `JSC::JSValue` sem V8).

**O que o RTS constrói:** uma camada de marshalling — `napi_value` vira um
**handle estável** (índice na `HandleTable`/handle scope), nunca um ponteiro cru
para um `RuntimeValue` (senão o sweep coleta no meio da chamada — bug "handle
collected before use" já documentado no RTS). Boxing/desboxing:
- `number` → box `i64`/`f64`;
- `string` → handle do pool GC (`gc::string_*`);
- `object`/`array` → handle `collections.map_*`/`vec_*`;
- ponteiro nativo → `napi_external` (handle `u64`).

**Vantagem do RTS sobre o Bun aqui:** o GC do RTS é **não-móvel** (mark+sweep,
não copia/realoca). No V8 (GC móvel) o handle scope é obrigatório para o GC
atualizar ponteiros; no RTS um `napi_value` pode ser um handle estável sem
realocação — simplifica a semântica.

---

## Divergência 2 — ABI e loading 🟠

Duas partes.

**(a) Implementar a superfície N-API.** O RTS teria que exportar **~150
funções** `napi_*`/`node_api_*` como `extern "C"` reais do runtime (a contagem
real é **~110-160**: `node-api-headers` v10 ~111, headers do Node ~161, Bun
"156/156", Deno 163 incl. libuv). É o **maior volume de trabalho puro**, mas:
- **não há bloqueador conceitual** — cada `napi_*` traduz para primitivas RTS
  (`gc.*`, `collections.map_*`, `string.*`);
- o RTS **já** tem o paradigma `extern "C"` tipado (40+ namespaces, símbolos
  `__RTS_FN_*`), então +~150 símbolos `extern "C"` é **natural ao modelo**;
- o esforço é **alto mas linear/incremental** (o Bun provou que dá, sem V8).

**(b) Carregar dinamicamente.** O RTS precisa de `libloading` (dlopen/
LoadLibrary) para abrir o `.node`, resolver `napi_register_module_v1`, e fabricar
um `napi_env` (ponteiro para uma `RtsNapiEnv`: `HandleTable`, handle-scope stack,
slot de exceção, ponte tokio). Trabalho direto.

**Atrito com o modelo do RTS:** o `.rtslib` (proposta existente) é **link
estático**, tipos de máquina, símbolo direto. O `.node` é **dlopen dinâmico** com
indirect call. Suportar `.node` é **construir um SEGUNDO loader dinâmico ao lado
do estático**, não estender o existente. Ver Divergência 5.

**Ponto de integração concreto no código:**
`crates/rts-codegen/src/module/import_resolver.rs::resolve_node_modules_import`
hoje só aceita `.rts/.ts/.js` (a função `resolve_source_candidate` rejeita outras
extensões). Um `.node` (ou um `package.json` cujo `main` aponta para `.node`)
seria **interceptado ali** e roteado para o loader N-API em vez do pipeline de
compilação TS.

---

## Divergência 3 — GC e finalizers 🟡

**O problema.** `napi_ref`/`napi_wrap`/`napi_add_finalizer` atrelam lifetime e
finalização ao GC do V8. O RTS tem mark+sweep com stack maps Cranelift.

**Sub-problema crítico — handle scopes invisíveis ao stack map.** Um `napi_value`
vivo dentro de um addon C **não aparece** no stack map Cranelift (o frame é do
addon, código nativo opaco ao RTS). Logo `mark_stack_roots()` não o veria e o
sweep o coletaria no meio da chamada.

**O que o RTS constrói:**
1. **Handle scopes como roots extras:** cada handle scope é um vetor de handles
   registrado como raiz adicional no scanner do GC (igual ao Bun: array de slots
   escaneável); fechar o scope desregistra.
2. **`napi_ref` com refcount:** strong (refcount > 0) conta como GC root; weak
   (0) **não** marca, mas guarda o handle para retornar `null` pós-coleta.
3. **Finalizers no sweep:** ao liberar um `Entry` com finalizer N-API associado,
   `sweep_all_shards()` **enfileira** a chamada do `napi_finalize` — executada
   **fora** da fase de marcação (timing "second-pass": chamar o motor durante o
   weak callback é inseguro).

**Estado do RTS:** já reconhece a dificuldade de weak refs — issue **#217**
(WeakMap/WeakSet hoje com semântica forte). A integração N-API é a mesma família
de problema. É engenharia GC real e ordenada, mas **conhecida — não bloqueador**.

---

## Divergência 4 — Event loop / async 🟠

**O problema.** `napi_create_async_work`/`napi_queue_async_work` e threadsafe
functions assumem o **loop libuv**. `napi_get_uv_event_loop` devolve um
`uv_loop_t*` cru. O RTS usa **tokio**.

**O que o RTS constrói:**
- `napi_create_async_work` → `rt().spawn_blocking(execute_cb)`; `complete_cb`
  postado de volta à thread que executa JS (modelo `promise.create`/#437);
- threadsafe function → fila MPSC drenada na thread JS;
- `napi_get_uv_event_loop` → o ponto **mais espinhoso**: addons que linkam libuv
  direto e usam `uv_async_t` no loop cru exigiriam um **shim `uv_loop_t` mínimo**
  sobre tokio (o Bun exporta só um subset de `uv_*`).

**Por que é a área de maior risco.** É **cauda longa**: o caminho síncrono
(maioria dos addons utilitários) não toca nisso e funciona sem o loop. Mas o
shim `uv_loop` cru é onde **o próprio Bun ainda tem gaps**. Liga-se à issue
**#207** do RTS (real async event loop ainda aberta) — o gargalo.

**Não bloqueador**, mas o item async deve ser **fase tardia** com mensagem de
erro clara quando um símbolo `uv_*` não suportado for chamado (como o Bun faz).

---

## Divergência 5 — JIT vs AOT 🔴 (filosófico)

**O problema.** No modo **AOT** (`rts compile`) o RTS produz um binário nativo
**self-contained** (a promessa do `.rtslib`: "um binário, zero arquivos, sem
carregamento dinâmico"). Mas um `.node` de terceiros é uma **shared lib
relocável** que **não** pode ser linkada estaticamente como um `.o` — ela espera
resolver `napi_*` do host em runtime via tabela de símbolos dinâmica.

**A única saída (precedente Deno).** `deno compile` historicamente **falhava**
com addons (`#23266`). A solução (`#28934`, `deno_rt_native_addon_loader`):
**embutir** o `.node` no binário e, no startup, **extrair para um tempdir +
dlopen**. Limitações admitidas: não funciona em FS **read-only** nem se a lib
precisa de outros arquivos reais em disco.

**Por que NÃO existe link estático de `.node`.** O SO precisa de um **arquivo em
disco** para `mmap`+relocar a shared lib — não há `dlopen` de código que está só
dentro do binário.

**Conclusão por modo:**
- **JIT (`rts run`):** já há memória executável e o processo já é dinâmico →
  `dlopen` é **natural e sem fricção**. ✅
- **AOT (`rts compile`):** ou **proibir** `.node` (preserva a pureza
  self-contained), ou adotar o modelo **self-extracting** explícito do Deno
  (quebra "um binário, zero arquivos"). ⚠️ Tradeoff arquitetural explícito que
  **contradiz o `.rtslib` estático**.

---

## Divergência extra — addons V8-diretos / NAN 🔴 (fora de escopo)

A **única candidata a bloqueador técnico** (parcial). Addons que linkam contra
`v8::*` (ou usam `nan`/`node-addon-api` que expandem para **inline V8**)
dependem do **layout binário do V8**: funções inline compiladas dentro do
`.node` fazem *raw field reads* em offsets fixos (tagged pointers, internal
fields) que o host **não pode interceptar** (ver
[`03-acoplamento-v8-libuv.md`](03-acoplamento-v8-libuv.md) §3.3).

Para suportá-los, o RTS teria que **sintetizar um ABI binário V8 fake** —
esforço imenso e frágil (o Bun documenta como multi-parte e incompleto; o Deno
nem com V8 real roda `better-sqlite3`).

**Recomendação:** o RTS deve mirar **APENAS** `.node` compilados contra Node-API
estável (a maioria moderna do npm via `napi-rs`/`node-addon-api` em modo N-API) e
**declarar addons v8-diretos como não-suportados** — exatamente a postura do Bun
e do Deno.

---

## Quadro de pontos de integração com o RTS existente

| Peça N-API | Infra RTS que reaproveita | Issue/spec relacionada |
|---|---|---|
| `napi_value` ↔ handle | `HandleTable` (slab, gen+slot, 32 shards) | — |
| `napi_create_string_utf8` | pool de strings GC (`gc::string_*`) | bug #235 (string com `\0`) |
| `napi_create_object`/`set_named_property` | `collections.map_*` / `RuntimeValue::Object` | — |
| handle scopes como roots | `mark_stack_roots()` / `thread_registry` | — |
| `napi_ref` weak / finalizers | `sweep_all_shards()` + refcount | #217 (weak WeakMap/Set) |
| `napi_create_promise` | `promise.create` / `PromiseAsync` | #437 (async/Promise) |
| async work / TSFN | `async_rt::rt()` / `spawn_blocking` / `tokio_ctx` | #207 (event loop) |
| `napi_callback` (CDECL) | trampolim de troca de callconv | precedente `invoke_all_i64` |
| `napi_create_external` / buffers | namespace `buffer` / `Vec<u8>` na HandleTable | — |
| `napi_define_class` (classe dinâmica em runtime) | classes RTS hoje só em compile-time | ⚠️ ponto que mais aperta |
| `napi_throw` / exceção pendente | error slot thread-local (try/catch fase 1) | #128 |

## Conclusão do capítulo

- **Nenhum bloqueador absoluto.** As barreiras reais são **volume** (Div. 2),
  **cauda longa async** (Div. 4) e **conflito filosófico AOT** (Div. 5).
- O **único quase-bloqueador técnico** (V8-direto/NAN) se resolve **restringindo
  o escopo a N-API puro** — postura validada por Bun e Deno.
- A maioria das peças tem **infra RTS reaproveitável** (`HandleTable`, GC,
  promise, tokio). O ponto que mais aperta é `napi_define_class` (classe dinâmica
  em runtime, que o RTS hoje só faz em compile-time) — evitável na fase 1
  mirando addons que **só exportam funções**.

→ Estratégia e roadmap em [`06-estrategia-roadmap.md`](06-estrategia-roadmap.md).

## Fontes

- https://nodejs.org/api/n-api.html · https://nodejs.org/en/learn/modules/abi-stability
- https://bun.com/docs/runtime/node-api · /blog/how-bun-supports-v8-apis-without-using-v8-part-1 · part-2
- https://github.com/oven-sh/bun/issues/158 · /23136
- https://github.com/denoland/deno/issues/23266 · /pull/28934
- https://github.com/nodejs/node-addon-api/blob/main/doc/external_buffer.md
