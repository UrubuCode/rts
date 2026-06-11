# 03 — Acoplamento com V8, libuv e o runtime host

> O que um host precisa prover para um `.node` carregar e rodar. Verificado
> contra `nodejs.org/api/addons.html`, `nodejs.org/api/n-api.html`,
> `node-gyp/src/win_delay_load_hook.cc`, issues do Node/Bun e o blog técnico do
> Bun.

## 3.1 Um `.node` NÃO é self-contained — tem símbolos undefined

Um addon é compilado deixando símbolos como `napi_*`, `uv_*`, `v8::*`, `node::*`
como **undefined**; o linker dinâmico os resolve a partir do **binário host** já
carregado em memória (node.exe / electron.exe / o próprio RTS.exe).

Doc oficial (confirmada verbatim):
> "Only the libuv, OpenSSL, V8, and zlib symbols are purposefully re-exported by
> Node.js and may be used to various extents by addons."

Issue `nodejs/node#52282`: quando o Node é compilado como shared library, um
`node.def` lista todos os símbolos exportados, e qualquer host que embute a lib
precisa linkar contra `node.def` para "re-exportar os símbolos necessários" que
o `.node` espera.

**Requisito central para o RTS:** para carregar um `.node`, o RTS **não** precisa
parsear/relinkar nada — precisa apenas:
1. `dlopen` do `.node`, e
2. **exportar** do seu próprio executável a tabela de símbolos `napi_*` (e, p/
   async, um subconjunto `uv_*`) que o `.node` referencia como undefined.

Sem essa exportação, o `dlopen` falha com *undefined symbol*.

## 3.2 Quanto um addon depende de V8 vs apenas N-API

Três vias (acoplamento decrescente — ver
[`01-formato-binario.md`](01-formato-binario.md) §1.8):

| Via | Depende de | ABI estável? | Suportável no RTS? |
|---|---|---|---|
| **Node-API** (C) | só símbolos `napi_*` | ✅ sim | ✅ **sim** |
| **node-addon-api** (C++ header-only) | reduz a símbolos `napi_*` | ✅ sim (herda) | ✅ **sim** |
| **nan** (C++ sobre V8) | `v8::Local`, `v8::Isolate`, `v8::HandleScope`… | ❌ não | ❌ caro |
| **V8 direto** (`v8.h`) | layout binário do V8 | ❌ não | ❌ inviável |

A garantia de ABI vale **apenas** para `<node_api.h>`. A doc afirma que
`<node.h>`, `<node_buffer.h>`, `<uv.h>`, `<v8.h>` **não** têm estabilidade ABI
entre majors.

## 3.3 Por que addons V8-diretos/NAN são o caso difícil

O problema não é só "exportar símbolos V8". Funções **inline** dos headers do V8
são **compiladas DENTRO do `.node`** e o host não pode interceptá-las.

Blog do Bun (*how-bun-supports-v8-apis-without-using-v8*):
- `v8::Object::GetInternalField` é inline; "performs some checks and then calls
  `v8::Object::SlowGetInternalField` if they fail" — essas checagens
  desreferenciam ponteiros usando o **esquema de tagged pointers do V8** (lower 2
  bits = tipo);
- esse código "gets compiled into each native module. **We can't change any of
  it**";
- JSC usa **NaN-boxing** (51 bits em NaN, 48-bit heap ptr / 32-bit int) — layout
  incompatível com o tagged-pointer do V8.

Para suportar esses addons, o host teria que **emular o layout de memória do
V8** (tagged pointers, `HandleScope` com stack de slots que o GC varre, internal
fields em offsets fixos, "Maps" falsos de tipo V8). O Bun documenta isso como
trabalho **multi-parte e incompleto** (issue `oven-sh/bun#4290`).

**Conclusão para o RTS:** suportar N-API é tratável e bem-definido (~150 fns,
faixa ~110-160);
emular o ABI binário do V8 é um projeto separado, enorme e frágil. **O RTS deve
mirar SÓ N-API** e declarar addons v8-diretos/NAN como não-suportados — a mesma
fronteira que Bun levou anos empurrando e que Deno (mesmo com V8 real) também não
cruza (`better-sqlite3` falha nos dois).

## 3.4 Trabalho assíncrono nativo via libuv

Conjunto de funções:
- **async simples:** `napi_create_async_work`, `napi_delete_async_work`,
  `napi_queue_async_work`, `napi_cancel_async_work` — internamente
  `uv_queue_work(loop, req, work_cb, after_work_cb)` (threadpool do libuv,
  `after` roda na thread do loop);
- **thread-safe:** `napi_create_threadsafe_function` + `call/acquire/release/
  ref/unref` — usam um `uv_async_t` (`uv_async_send` acorda o loop de outra
  thread);
- **event loop:** `napi_get_uv_event_loop(env, uv_loop_t**)` — dá o loop cru.

Addons NAN/diretos chamam `uv_default_loop()` para pegar o loop.

**Divergência para o RTS:** o RTS roda **tokio**, não libuv. Para async N-API:
- `napi_create_async_work` → `rt().spawn_blocking(execute_cb)`, `complete_cb`
  postado de volta à "thread principal" RTS;
- threadsafe function → fila MPSC drenada na thread que executa JS (casa com o
  modelo `promise`/`spawn_blocking` do RTS, #437);
- `napi_get_uv_event_loop` é o ponto **mais espinhoso** — addons que usam
  `uv_async_t` no loop cru exigiriam um **shim `uv_loop_t` mínimo** sobre tokio.

O Bun **não** roda sobre libuv (Linux/macOS): `uv_default_loop()` não é o loop do
runtime, então addons que o usam direto quebram (issues `oven-sh/bun#14830`,
`#20453`, `#25220`); só funcionam via `napi_get_uv_event_loop` + um shim libuv.
Esta é a área onde **até o Bun ainda tem parity gaps**.

## 3.5 Resolução de símbolos no Windows: delay-load

`node-gyp` injeta `src/win_delay_load_hook.cc` em todo addon. O mecanismo:
- registra `__pfnDliNotifyHook2 = load_exe_hook`;
- quando o loader tenta carregar a DLL `HOST_BINARY` (ex.: `node.exe`), o hook
  intercepta em `dliNotePreLoadLibrary`, compara via `_stricmp(info->szDll,
  HOST_BINARY)`, e em vez de procurar o `.exe` no disco retorna
  `GetModuleHandle(TEXT("libnode.dll"))` ou, se nulo, **`GetModuleHandle(NULL)`**
  (handle do próprio processo);
- assim os símbolos `napi_*`/`uv_*`/`v8` são resolvidos a partir do executável
  host — **funciona mesmo se o host foi renomeado**.

**Para o RTS no Windows:** basta (1) **exportar** `napi_*`/`uv_*` do seu próprio
`.exe` (equivalente a um `node.def` / `/EXPORT`), e (2) o `win_delay_load_hook`
já embutido no `.node` cairá no fallback `GetModuleHandle(NULL)` → buscará os
símbolos **no RTS.exe**. O RTS só precisa garantir que esses símbolos estejam na
export table do binário.

## 3.6 Resolução de símbolos no Linux/macOS

Node carrega via `dlopen` (padrão `RTLD_LAZY|RTLD_LOCAL`, configurável via
`process.dlopen` flags + `os.constants.dlopen`). Em macOS `dlopen` age como
`RTLD_GLOBAL` por padrão; Linux usa `RTLD_LOCAL`. Os símbolos `napi_*`/`uv_*`/
`v8` undefined no `.node` são resolvidos contra o binário host já carregado.

**Para o RTS no Linux/macOS:** o `RTS.exe` precisa ser linkado com os símbolos
`napi_*`/`uv_*` **visíveis dinamicamente** — ou seja, **não** `-fvisibility=
hidden` para eles; provavelmente um *version script* / `--dynamic-list` ou
`--export-dynamic`. Sem isso, mesmo implementando `napi_*`, o `dlsym` dinâmico do
`.node` não os acha. **Detalhe de link a observar no RTS.**

## 3.7 Precedente: como Bun e Deno exportam os símbolos

- **Bun** (JSC, não-V8): implementa N-API do zero. `napi.cpp` (bindings JSC) +
  `napi.zig` (TSFN/async/loop) + `bindings/v8/` (shim V8 p/ NAN) +
  **`src/symbols.txt`** (lista de símbolos exportados). 156/156 fns. `napi_value`
  ↔ `JSC::JSValue`.
- **Deno** (V8 via rusty_v8): `napi_value` **é** um `v8::Local` de verdade
  (mapeamento direto). Crate `deno_napi`; símbolos declarados via macro
  `#[napi_sym::napi_sym]`, nomes em
  **`cli/napi_sym/symbol_exports.json`**, `.def` gerado por
  `tools/napi/generate_symbols_lists.js`. Loader via `libloading`. Mesmo entry
  point `napi_register_module_v1`.

**Para o RTS:** o molde é claro — uma **lista declarativa de símbolos N-API**
(análoga ao `symbol_exports.json` do Deno e à própria `abi::SPECS`/`symbols.rs`
do RTS), com geração de `.def`/export table. A maquinaria que o RTS já tem para
gerar `rts.d.ts` a partir de `SPECS` geraria os exports N-API.

## Conclusão do capítulo

- O host precisa **exportar `napi_*` (+ subset `uv_*`)** do seu binário e fazer
  `dlopen` + `dlsym(napi_register_module_v1)`.
- No Windows o delay-load hook cai em `GetModuleHandle(NULL)` → resolve no
  RTS.exe; no Linux/macOS exige `--export-dynamic` dos símbolos N-API.
- N-API é tratável; **V8-direto/NAN exige emular o layout binário do V8** —
  fora de escopo.
- async/libuv é o degrau difícil — ponte tokio + shim `uv_loop` mínimo (área
  com gaps até no Bun).

## Fontes

- https://nodejs.org/api/addons.html · /api/n-api.html
- https://github.com/nodejs/node-gyp/blob/main/src/win_delay_load_hook.cc
- https://github.com/nodejs/node/issues/52282
- https://github.com/oven-sh/bun/issues/158 · /4290 · /14830 · /20453 · /25220
- https://bun.com/blog/how-bun-supports-v8-apis-without-using-v8-part-1 · part-2
- https://docs.rs/deno_napi/latest/deno_napi/
- https://github.com/denoland/deno/tree/main/ext/napi
- https://docs.libuv.org/en/v1.x/async.html
- https://man7.org/linux/man-pages/man3/dlopen.3.html
