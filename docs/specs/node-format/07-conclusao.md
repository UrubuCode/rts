# 07 — Conclusão: viabilidade de suporte a `.node` no RTS

> Síntese executiva. Veredito de viabilidade, as decisões-chave e os próximos
> passos concretos.

## TL;DR

**Suportar `.node` no RTS é viável, mas só pela porta N-API, preferencialmente
no modo JIT, e nunca para addons V8-diretos/NAN.** Não há bloqueador técnico
absoluto — o Bun provou que dá para implementar N-API sobre um engine não-V8. As
barreiras reais são **volume de trabalho** (~150 funções, faixa ~110-160), a **cauda longa do
event loop** (libuv vs tokio) e um **conflito filosófico** entre o `dlopen`
dinâmico do `.node` e a promessa self-contained do modo AOT (`.rtslib`).

## Os 5 fatos que decidem tudo

1. **Um `.node` é uma DLL/`.so`/`.dylib` comum**, com extensão trocada; o ponto
   de entrada é a função exportada **`napi_register_module_v1(napi_env,
   napi_value) -> napi_value`**. (Verificado no código: `node_binding.cc`.)

2. **`napi_value` e `napi_env` são ponteiros opacos.** O addon nunca os
   dereferencia — só os passa de volta às funções `napi_*`. Isso permite ao RTS
   mapeá-los para sua **`HandleTable`/`RuntimeValue`** sem precisar de V8.
   (Verificado: `napi_value` = "an opaque pointer".)

3. **N-API é ABI-estável e independente do engine; o V8 cru NÃO é.** Addons
   N-API/`node-addon-api` são suportáveis; addons `nan`/`v8.h`-direto exigem
   **emular o layout binário do V8** (tagged pointers, internal fields inline no
   `.node`) — inviável. **Escopo = N-API puro.** (Postura idêntica de Bun e
   Deno; `better-sqlite3`/`bcrypt`/`canvas` falham nos dois.)

4. **Addons N-API pulam a checagem de `NODE_MODULE_VERSION` no load** — o RTS não
   precisa fingir uma versão de Node para o `dlopen`. Só precisa de
   `platform`/`arch` corretos para a **seleção do prebuilt** (prebuilds N-API
   modernos via `napi-rs`/`prebuildify` são por plataforma, não por versão de
   Node).

5. **O host precisa exportar `napi_*` (+ subset `uv_*`) do seu binário.** No
   Windows o `win_delay_load_hook` embutido no `.node` cai em
   `GetModuleHandle(NULL)` → resolve no RTS.exe; no Linux/macOS exige
   `--export-dynamic`. **O RTS não relinka o `.node` — só faz dlopen + dlsym e
   provê os símbolos.**

## Veredito por divergência

| Divergência | Veredito |
|---|---|
| 1. Representação de valor | 🟢 **Não bloqueador** — camada de marshalling `napi_value` ↔ handle (GC não-móvel do RTS até simplifica) |
| 2. ABI/loading | 🟡 **Engenharia alta por volume** — ~150 `extern "C"` (faixa ~110-160) + libloading; conceitualmente alinhado ao RTS |
| 3. GC/finalizers | 🟢 **Não bloqueador** — handle scopes como roots + finalizers no sweep; família do #217 |
| 4. Event loop async | 🟡 **Média-alta, cauda longa** — ponte tokio fácil; shim `uv_loop` cru difícil (gaps até no Bun); família do #207 |
| 5. JIT/AOT | 🔴 **Bloqueador filosófico no AOT** — natural no JIT; AOT exige self-extracting (quebra self-contained) |
| extra. V8-direto/NAN | 🔴 **Fora de escopo** — resolve-se restringindo a N-API puro |

## As decisões arquiteturais recomendadas

1. **Mirar SÓ N-API.** Declarar addons V8-diretos/NAN não-suportados, com
   mensagem de erro clara. (Validado por Bun e Deno.)
2. **Começar por JIT (`rts run`).** É onde `dlopen` é natural. No AOT: proibir
   inicialmente, ou self-extracting explícito depois.
3. **`napi_value` = indireção estável** (handle na `HandleTable`/handle scope),
   **nunca** ponteiro cru de `RuntimeValue` (evita "handle collected before
   use").
4. **Reaproveitar a infra existente:** `HandleTable`, GC mark+sweep, `promise`
   (#437), `async_rt`/tokio, error slot thread-local, convenção `symbols.rs` +
   geração a partir de `SPECS`.
5. **Exigir um flag de permissão** (`--allow-native-addons`) — código nativo fora
   do sandbox (lição do `--allow-ffi` do Deno).
6. **`.rtslib` e `.node` são complementares**, não substitutos: `.rtslib` =
   first-party performante (link estático, tipos de máquina); `.node` =
   compatibilidade com o ecossistema npm (dlopen dinâmico, N-API shim).

## Esforço realista

- **Núcleo 80/20 (Fase 0+1, ~40 fns síncronas, só JIT):** o primeiro addon real
  rodando é um marco **alcançável e bem-delimitado**.
- **N-API completa (Estratégia A):** **muitos meses** — o Bun levou anos e ainda
  está em ~76% nos suites do Node. É destino, não ponto de partida.
- **O degrau caro:** event loop async (Fase 3) — onde o próprio Bun tem gaps.

## Próximos passos concretos

1. **Validar o ponto de integração:** confirmar que
   `resolve_node_modules_import`
   (`crates/rts-codegen/src/module/import_resolver.rs`) é o lugar para
   interceptar `.node` (hoje rejeita extensões ≠ `.rts/.ts/.js`).
2. **PoC Fase 0:** loader `libloading` + `napi_register_module_v1` + `napi_env`
   mínimo + export-dynamic dos símbolos, rodando um addon N-API trivial em
   `rts run`. Usar `esbuild` ou um addon `napi-rs` "hello world" como alvo.
3. **Abrir um epic** com as 5 fases do [`06-estrategia-roadmap.md`](06-estrategia-roadmap.md),
   cada fase com um addon real como critério de saída.
4. **Definir a baseline de conformidade:** rodar as suites `js-native-api` e
   `node-api` do Node (como o Bun faz no CI) para medir paridade — integra ao
   sistema de fixtures cross-runtime que o RTS já tem.
5. **Decidir a política AOT:** proibir `.node` em `rts compile` (preserva
   `.rtslib`) ou planejar self-extracting — decisão de produto, não técnica.

## Mapa dos documentos

- [`01-formato-binario.md`](01-formato-binario.md) — o que é o `.node`, loader,
  símbolos, `NODE_MODULE_VERSION`.
- [`02-napi-abi.md`](02-napi-abi.md) — a ABI N-API: valores, env, scopes, refs,
  o núcleo de funções.
- [`03-acoplamento-v8-libuv.md`](03-acoplamento-v8-libuv.md) — símbolos que o
  host exporta, delay-load, libuv.
- [`04-precedente-bun-deno.md`](04-precedente-bun-deno.md) — como Bun e Deno
  fizeram; o que quebra; custo.
- [`05-divergencias-rts.md`](05-divergencias-rts.md) — as 5 divergências
  classificadas, pontos de integração.
- [`06-estrategia-roadmap.md`](06-estrategia-roadmap.md) — estratégias, 80/20,
  prebuilds, roadmap faseado.

## Nota de confiança e metodologia

Este estudo foi produzido por uma pesquisa multi-agente (6 eixos paralelos com
busca web) sobre fontes **primárias**: documentação oficial do Node
(`n-api.html`, `addons.html`), código-fonte do Node (`node_binding.cc`,
`node.h`, `node_api.h`, `node_version.h`, `js_native_api_*.h`), código e blog do
Bun, e `deno_napi`/issues do Deno.

**Verificação adversarial (dois runs independentes completos):** cada afirmação
técnica estrutural passou por um verificador cético que tentou refutá-la contra
fontes primárias. Resultado agregado dos dois runs:

| Métrica | Run A | Run B |
|---|---|---|
| Achados de pesquisa | 60 | 76 |
| Afirmações verificadas | 50 | 65 |
| **Confirmadas** | 35 | 49 |
| **Parciais** (nuance, não invalidação) | 15 | 16 |
| **Refutadas** | **0** | **0** |
| Não-verificadas (rate-limit/limite de sessão) | 4 | 4 |

**Zero refutações em 115 verificações.** Os ~31 veredictos "parciais" foram todos
**correções de nuance** já incorporadas aos documentos:

- `NODE_MODULE_VERSION 147` = **Node 26** (não 27) — corrigido em §1.7.
- A invocação N-API usa `napi_module_register_by_symbol` com **5 args**
  (`+module_api_version`) no Node atual — §1.3.
- `NODE_C_CTOR` no MSVC usa construtor de struct estática, **não** `.CRT$XCU` — §1.4.
- Superfície N-API = **~110-160 funções** (não um "156" fixo) — §2.5, doc 05/06.
- Caminhos atuais do Bun: `src/jsc/bindings/napi.cpp` + `src/runtime/napi/napi.zig`
  + `src/jsc/bindings/v8/` (não `src/bun.js/bindings/...`) — doc 04.
- `napi_value` no Deno = newtype `#[repr(transparent)]
  NapiValue(Option<NonNull<v8::Value>>)`, nullable — doc 04.
- O GC do JSC é **non-moving** (Riptide); a falha da impl. ingênua do Bun é
  **rooting/visibilidade ao GC**, não movimento — doc 04.
- `bcrypt` migrou para N-API (v4.0.0) e funciona no Bun — doc 04 §4.4.

Os fatos centrais foram confirmados **verbatim contra o código-fonte do Node**:
`napi_register_module_v1`, `node_register_module_v<N>`, a mensagem de mismatch de
`NODE_MODULE_VERSION`, `thread_local_modpending`, e a checagem
`if ((mp->nm_version != -1) && (mp->nm_version != NODE_MODULE_VERSION))` com o
comentário `// -1 is used for Node-API modules` (que prova a isenção N-API).
