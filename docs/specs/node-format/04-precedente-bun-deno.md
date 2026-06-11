# 04 — Precedente: como Bun e Deno implementaram N-API

> O roadmap que o RTS seguiria. Bun (sobre JavaScriptCore, **não**-V8) é o
> precedente mais próximo do RTS; Deno (sobre V8 real) mostra o contraste.
> Verificado contra blog/docs do Bun, `deno_napi`, e issues do GitHub.

## 4.1 O contrato é idêntico para os dois: `napi_register_module_v1`

Ambos carregam o `.node` via `dlopen`/`libloading`, resolvem
`napi_register_module_v1(napi_env, napi_value exports) -> napi_value`, fabricam
um `napi_env`, criam um `exports` vazio e chamam a init. O erro quando o símbolo
falta (visto em runtimes não-Node):
> `symbol napi_register_module_v1 not found in native module. Is this a Node API (napi) module?`

## 4.2 Deno — o caminho "fácil" (engine V8)

- Embute V8 via **rusty_v8**; logo `napi_value` **é** um `v8::Local` de verdade —
  **sem tradução de engine**. `deno_napi` só faz transmute/abstração.
- Estrutura: crate `deno_napi` com `Env`, `EnvShared`, `NapiModule`, `NapiState`,
  `InstanceData`; módulos `js_native_api` (operações de valor), `node_api`,
  `function`, `value`, `uv` (integração libuv).
- Símbolos: macro `#[napi_sym::napi_sym]`, nomes em
  `cli/napi_sym/symbol_exports.json`, `.def` gerado por
  `tools/napi/generate_symbols_lists.js`. Loader via `libloading`
  (`DenoRtNativeAddonLoader`).
- **Modelo de permissão:** addons exigem `node_modules/` local + flag
  **`--allow-ffi`** (mesma permissão do FFI cru, pois roda fora do sandbox;
  aceita lista de paths `--allow-ffi=./libfoo.so`).
- **`deno compile`:** historicamente falhava com addons (issue `#23266`,
  `LoadLibraryExW failed`). PR `#28934` adicionou
  `deno_rt_native_addon_loader` que **extrai** as shared libs / `.node` embutidos
  **para um arquivo temp** e os abre dali (modelo self-extracting). Limitações
  admitidas: não funciona em FS read-only nem se a lib precisa de outros arquivos
  reais em disco.

> **Nuance verificada (`napi_value` no Deno):** não é *literalmente* um
> `v8::Local`. Em `ext/napi/value.rs` é um newtype
> `#[repr(transparent)] NapiValue<'s>(Option<NonNull<v8::Value>>)` —
> **nullable** (≠ o `Local` não-nullable do rusty_v8), conversível para/de
> `v8::Local` via `From` e `Deref`/`transmute` **sem tradução de engine** (porque
> o engine *é* V8). Ponte quase zero-custo, mais barata que a do Bun.
> A flag `deno compile --self-extracting` (validada no Deno 2.3) é o mecanismo de
> AOT.

**Lição para o RTS:** Deno teve o caminho fácil para `napi_value` (tem V8). O RTS
**não** está nesse caminho — segue o modelo **Bun** (mapear `napi_value` para
handle próprio). Mas o **modelo de permissão** (`--allow-ffi`) e o **modelo
self-extracting** para o AOT são diretamente reaproveitáveis.

## 4.3 Bun — o caminho do RTS (engine não-V8)

- Usa **JavaScriptCore**; trata `napi_value` como um `JSC::JSValue`
  (`EncodedJSValue`, 8 bytes, **NaN-boxing**, ≠ tagged pointer do V8), via
  `reinterpret_cast` + `JSValue::encode/decode`. O NaN-boxing do JSC usa uma
  **tag de 16 bits superiores** (não "2 bits"): tag `0x0000` = ponteiro `JSCell`
  de 48 bits (objetos no heap); tag `0xFFFE` = `int32` com sinal (ex.:
  `0xfffe000000000000 | (uint32_t)input`).
- **Cobertura COMPLETA de superfície:** issue `oven-sh/bun#158` reporta
  **156/156** funções `napi_*`/`node_api_*` implementadas e exportadas. O
  restante é **paridade comportamental (~95%)**, não API faltando.
- Roda as suites oficiais do Node no CI: `js-native-api` (engine-agnóstico,
  ~98-100%), `node-api` (específico-Node, ~48%), combinado ~76%.
- Estrutura de código (caminhos **atuais** do repo — molde para o RTS):
  - `src/jsc/bindings/napi.cpp` — funções `napi_*` + integração JSC (valor);
  - `src/runtime/napi/napi.zig` — threadsafe functions, async work, event loop,
    handle scopes;
  - `src/jsc/bindings/v8/` — shim do ABI C++ do V8 (para addons NAN);
  - `src/symbols.txt` — lista de símbolos exportados.
  > (Caminhos antigos `src/bun.js/bindings/...` aparecem em material datado.)

### Como Bun gerencia handle scopes (modelo direto p/ o RTS)

`HandleScope` de 24 bytes stack-alocado { ptr Isolate, ptr scope anterior, ptr
`HandleScopeBuffer` }; no construtor se marca como *current*, no destrutor
restaura o anterior. O scope atual fica em
`globalObject->V8GlobalInternals()->currentHandleScope()` (lazily pendurado no
global object). Cada `Handle` = 24 bytes. `napi_value` retornado é ponteiro para
o slot **estável** no buffer — **nunca** o ponteiro do objeto direto.

**Lição crítica para o RTS:** `napi_value` deve ser uma **indireção estável**
(slot na `HandleTable`/handle scope), nunca o ponteiro cru de um `RuntimeValue` —
senão o mark+sweep do RTS pode coletar/invalidar o valor no meio da chamada
nativa (a classe de bug "handle collected before use" que o RTS já documentou).

> **Nuance verificada (por que a implementação ingênua do Bun falha):** o GC do
> JSC é **non-moving** (Riptide, non-compacting — `JSCell`s ficam em endereço
> fixo), então o problema **não** é ponteiro invalidado por coleta movente. A
> implementação ingênua (partes 1-2 do blog) é "deeply broken" porque os
> `Handle`s vivem num `HandleScopeBuffer` que o coletor do JSC **não rastreia** —
> objetos referenciados só por handles podem ser coletados enquanto ainda vivos
> (problema de **rooting/visibilidade ao GC**, não de movimento). É exatamente o
> risco análogo no RTS: o handle scope precisa ser registrado como **root
> escaneável** pelo `mark_stack_roots()`, senão o sweep coleta o que o addon
> ainda usa. O GC não-móvel do RTS (como o do JSC) facilita a parte de
> *estabilidade de endereço*, mas **não** dispensa o rooting.

### O degrau difícil que o Bun não cruza por completo

- **async_hooks** não integrado (Bun recomenda `AsyncLocalStorage`);
- **libuv:** só um **subconjunto** de símbolos `uv_*` exportado;
  `uv_default_loop()` ≠ loop do runtime → addons que o usam direto quebram;
- ordenação de finalizers em *worker terminate* diverge;
- `napi_adjust_external_memory` difere do V8.

### O shim V8-C++ (NAN) — projeto separado, anos de trabalho

Issue `oven-sh/bun#4290`: para NAN/V8-direto, Bun shimou (desde **v1.1.25**,
ago/2024) `v8::Isolate::GetCurrent`, `HandleScope`, `EscapableHandleScope`,
`Number/String/Boolean/Object/Array/External`, `FunctionTemplate`,
`ObjectTemplate`, internal fields, `node_module_register`, cleanup hooks. **Ainda
quebram:** operações avançadas de Array, `ObjectTemplate` com construtores, casos
de `FunctionTemplate`, `ArrayBuffer`, strings UTF-16, persistent handles. A
tentativa ingênua de reinterpretar `Local<T>` ↔ `JSValue` **falhou** porque
funções inline do V8 dereferenciam assumindo o layout V8.

## 4.4 Mapa de compatibilidade de addons reais (baseline de teste)

| Addon | Bun | Deno | Por quê |
|---|---|---|---|
| `esbuild` | ✅ | ✅ | N-API puro |
| `sqlite3` (npm) / `duckdb` | ✅ | ✅ | N-API |
| `better-sqlite3` | ⚠️ contornado por `bun:sqlite` | ❌ (`#18444`, não acha bindings) | addon nativo; Deno usa `libsql`/`denodrivers` |
| `sharp` | ✅ (libvips + fallback WASM, 2026) | parcial | N-API + binário |
| `bcrypt` | ✅ (N-API desde v4.0.0; Bun v1.0.19+) | parcial | migrou de V8-C++ → N-API; `bcryptjs`/`Bun.password` ficam como conveniência |
| `node-canvas` (v2) | ❌ | ❌ | V8-C++ |

> **Nuance verificada (estado 2025-2026):** `bcrypt` **não** é mais "V8-C++ que
> quebra" — migrou para N-API na v4.0.0 e o Bun v1.0.19 desbloqueou o suporte
> oficial. O exemplo canônico de "quebra por V8-C++ direto" é o `node-canvas`
> (v2). `better-sqlite3` é addon nativo mas o atrito no Deno é não achar o
> arquivo de bindings, não V8-C++.

**Padrão geral:** addons **N-API puros** tendem a funcionar; addons que usam
**nan/v8.h direto** quebram nos dois runtimes — confirmando a fronteira que o RTS
deve adotar.

**Estratégia "API embutida em vez de addon":** o Bun oferece `bun:sqlite`
(4-6× mais rápido que `better-sqlite3`, evita marshaling N-API). O RTS já tem
namespaces nativos (`crypto`, `net`, `sqlite` futuro?) que podem servir de
alternativa quando o addon N-API for inviável.

## 4.5 Estimativa de esforço (do precedente)

- **Bun:** superfície N-API "completa" (156/156 fns na contagem da issue #158),
  mas **paridade comportamental +
  shim V8-C++** consumiram releases desde v1.1.25 (ago/2024) até hoje, com **três
  posts de blog** só explicando a emulação de layout. Combinado ainda **76%** nos
  suites do Node.
- **Deno:** macro `napi_sym` + lista JSON de símbolos; suporte **estável** no
  Deno 2.0; organização limpa (`js_native_api.rs`/`node_api.rs`/`function.rs`/
  `value.rs`/`uv.rs`).

**Ordem de grandeza para o RTS:** a Estratégia A completa (toda a N-API) é
**muitos meses**. Mas o **núcleo 80/20** (~40 fns síncronas) roda a maioria dos
addons N-API simples e é bem menor — ver [`06-estrategia-roadmap.md`](06-estrategia-roadmap.md).

## Conclusão do capítulo

- O RTS segue o **molde Bun** (engine não-V8 → mapear `napi_value` a handle
  próprio), com a organização de código do Deno (lista declarativa de símbolos).
- `napi_value` = **indireção estável** (handle), nunca ponteiro cru — alinhado à
  `HandleTable` do RTS.
- Escopo realista = **N-API puro**; V8-C++/NAN fora (a fronteira que nem Bun nem
  Deno cruzam por completo).
- Baseline de teste pronto: `esbuild`, `sqlite3`, `sharp` (sim); `bcrypt`,
  `better-sqlite3`, `canvas` (não) — serve para medir paridade, como o RTS já
  faz com fixtures cross-runtime.

## Fontes

- https://bun.com/blog/how-bun-supports-v8-apis-without-using-v8-part-1 · part-2
- https://bun.com/docs/runtime/node-api · /sqlite
- https://github.com/oven-sh/bun/issues/158 · /4290 · /16050
- https://docs.rs/deno_napi/latest/deno_napi/
- https://github.com/denoland/deno/tree/main/ext/napi
- https://github.com/denoland/deno/issues/18444 · /23266 · /pull/28934
- https://docs.deno.com/runtime/fundamentals/ffi/ · /node/
- https://www.alexcloudstar.com/blog/bun-compatibility-2026-npm-nodejs-nextjs/
