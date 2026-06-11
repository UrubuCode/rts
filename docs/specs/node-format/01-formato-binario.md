# 01 — O que é fisicamente um `.node` e como o Node o carrega

> Verificado contra fontes primárias: `nodejs.org/api/addons.html`,
> `nodejs.org/api/n-api.html`, e o código-fonte do Node
> (`src/node.h`, `src/node_api.h`, `src/node_version.h`, `src/node_binding.cc`).

## 1.1 Um `.node` é uma biblioteca dinâmica nativa — só a extensão muda

Não existe formato de container próprio. Um `.node` é literalmente a shared
library nativa do SO, apenas com a extensão renomeada:

| Plataforma | Formato real | Magic | Observação |
|---|---|---|---|
| Linux | ELF shared object (`.so`) | `0x7F 45 4C 46` (`\x7FELF`) | `readelf -h foo.node` funciona |
| Windows | PE/COFF DLL | `MZ` (`0x4D5A`) → `PE\0\0` | é literalmente um `.dll` |
| macOS | Mach-O dylib | `0xFEEDFACF` (64-bit) / `0xCAFEBABE` (fat) | é literalmente um `.dylib` |

A doc oficial confirma textualmente:

> "The filename extension of the compiled addon binary is `.node` (as opposed to
> `.dll` or `.so`). The `require()` function is written to look for files with
> the `.node` file extension and initialize those as dynamically-linked
> libraries."
> — `nodejs.org/api/addons.html`

> "*Addons* are dynamically-linked shared objects that can be loaded via the
> `require()` function as ordinary Node.js modules."

**Implicação para o RTS:** para *produzir* um `.node`, o RTS precisa emitir o
formato nativo do alvo (ELF `.so` / PE DLL / Mach-O dylib) como **shared
object**, não como executável — reusando o pipeline AOT (`cranelift_object` +
linker), mas linkando em modo *shared library* com tabela de exportação. Para
*consumir*, o loader é o do runtime host (o próprio RTS).

## 1.2 O fluxo de carregamento

```
require('./addon.node')
  → Module._extensions['.node']
  → process.dlopen(module, path.toNamespacedPath(filename))
  → internalBinding (src/node_binding.cc)
  → DLib::Open
      • POSIX:   dlopen(filename, RTLD_LAZY)        [flags configuráveis via os.constants.dlopen]
      • Windows: uv_dlopen(filename, &lib)
  → DLib::GetSymbolAddress(name)  → dlsym / uv_dlsym
  → invoca o símbolo de inicialização
  → o objeto `exports` populado vira o module.exports retornado ao JS
```

`DLib::Close` chama `dlclose`. O addon **não tem `main`**: ele só preenche e
devolve `exports`.

## 1.3 Os DOIS símbolos de entrada (confirmados no código)

`src/node_binding.cc` tem duas funções de descoberta. Verificado literalmente
contra o fonte:

**N-API (alvo do RTS):**
```c
inline napi_addon_register_func GetNapiInitializerCallback(DLib* dlib) {
  const char* name =
      STRINGIFY(NAPI_MODULE_INITIALIZER_BASE) STRINGIFY(NAPI_MODULE_VERSION);
  // NAPI_MODULE_INITIALIZER_BASE = "napi_register_module_v", NAPI_MODULE_VERSION = 1
  // → símbolo exato: napi_register_module_v1
```
Assinatura (extern "C", **sem mangling** — qualquer mangling impede o `dlsym`):
`napi_value napi_register_module_v1(napi_env env, napi_value exports)`. O Node
resolve via `dlib->GetSymbolAddress(name)` + `reinterpret_cast` e o invoca via
`napi_module_register_by_symbol(exports, module, context, init, module_api_version)`
— **cinco argumentos** no Node atual (o `module_api_version` foi adicionado; a
forma de 4 args vale só em versões antigas).

**Legado / raw-V8 (fora do alcance do RTS):**
```c
inline InitializerCallback GetInitializerCallback(DLib* dlib) {
  const char* name = "node_register_module_v" STRINGIFY(NODE_MODULE_VERSION);
  // → ex.: node_register_module_v147 (NODE_MODULE_VERSION 147 = ABI do Node 26;
  //   o branch main está em NODE_MAJOR_VERSION 27 em desenvolvimento)
```

A prova empírica de que runtimes alternativos procuram **exatamente**
`napi_register_module_v1` é a mensagem de erro real do Bun (issues
`oven-sh/bun#5578`, `#23136`, `#21432`):

> `TypeError: symbol napi_register_module_v1 not found in native module. Is this a Node API (napi) module?`

## 1.4 Auto-registro legado por constructor (caminho que o RTS ignora)

Addons legados (`NODE_MODULE`, NAN) se auto-registram via constructor estático
**antes** de qualquer símbolo ser procurado. `src/node.h`:

```c
#define NODE_MODULE_X(modname, regfunc, priv, flags)                  \
  static node::node_module _module = {                                \
      NODE_MODULE_VERSION, flags, NULL, __FILE__,                     \
      (node::addon_register_func)(regfunc), NULL,                     \
      NODE_STRINGIFY(modname), priv, NULL };                          \
  NODE_C_CTOR(_register_##modname) { node_module_register(&_module); }
```

`node_module_register` grava o módulo numa thread-local
(`thread_local node_module* thread_local_modpending;`), que o `DLOpen` lê após
o `dlopen`:

```c
CHECK_NULL(thread_local_modpending);
// ...
node_module* mp = thread_local_modpending;
thread_local_modpending = nullptr;
```

> **Correção verificada (detalhe MSVC):** `NODE_C_CTOR` expande para
> `__attribute__((constructor))` no GCC/POSIX, mas no MSVC **não** usa a seção
> `.CRT$XCU` — usa um construtor de struct estática em namespace anônimo
> (`struct fn##_ { fn##_(){ fn(); }; } fn##_v_;`), inicialização estática C++
> comum. (Apontado pela verificação adversarial.)

## 1.5 A struct `node_module` (legado) é acoplada a V8 → inviável no RTS

`src/node.h`:
```c
struct node_module {
  int nm_version;
  unsigned int nm_flags;
  void* nm_dso_handle;
  const char* nm_filename;
  node::addon_register_func nm_register_func;
  node::addon_context_register_func nm_context_register_func;
  const char* nm_modname;
  void* nm_priv;
  struct node_module* nm_link;
};
typedef void (*addon_register_func)(
    v8::Local<v8::Object> exports,   // ← tipos V8 no boundary
    v8::Local<v8::Value> module, void* priv);
```

O register legado recebe `v8::Local<v8::Object>` — **acoplamento direto ao V8**.
O RTS não tem V8, então o caminho legado é inviável de implementar fielmente.

## 1.6 A struct `napi_module` (N-API) é independente de V8 → viável

`src/node_api.h`:
```c
typedef struct napi_module {
  int nm_version;
  unsigned int nm_flags;
  const char* nm_filename;
  napi_addon_register_func nm_register_func;   // napi_value(*)(napi_env, napi_value)
  const char* nm_modname;
  void* nm_priv;
  void* reserved[4];
} napi_module;
```

`napi_env` e `napi_value` são **ponteiros opacos** — nenhum tipo V8 no boundary.
Isto é o que torna o suporte viável sem V8 (ver [`02-napi-abi.md`](02-napi-abi.md)).

A macro `NAPI_MODULE_INIT()` exporta dois símbolos com visibilidade default
(`__declspec(dllexport)` no Windows / `visibility("default")` no POSIX):
- `napi_register_module_v1(napi_env, napi_value) -> napi_value` (obrigatório)
- `node_api_module_get_api_version_v1(void) -> int32_t` (negociação de versão)

## 1.7 `NODE_MODULE_VERSION` — trava de ABI do caminho **legado** apenas

`src/node_version.h`: `#define NODE_MODULE_VERSION 147`. Pelo
`doc/abi_version_registry.json` oficial, **147 corresponde ao Node 26** (variant
v8_14.6); o valor aparece no `node_version.h` do branch `main` enquanto
`NODE_MAJOR_VERSION` está em 27 (desenvolvimento), mas a ABI foi registrada para
o Node 26. O Node recusa addons legados compilados contra ABI diferente:

```
"The module '%s' was compiled against a different Node.js version using
NODE_MODULE_VERSION %d. This version of Node.js requires NODE_MODULE_VERSION %d."
```

Valores confirmados: Node 16=93, 18=108, 20=115, 21=120, 22=127, 23=131, 24=137,
**26=147**. Registro canônico: `doc/abi_version_registry.json`. Exposto em JS como
`process.versions.modules`.

**Ponto decisivo:** addons **N-API pulam essa checagem** (a struct `napi_module`
carrega o seu próprio `NAPI_MODULE_VERSION` fixo e a verificação é diferente).
Confirmado empiricamente: o Bun reporta `NODE_MODULE_VERSION 127` e a checagem
só morde addons **não-N-API** (issue `oven-sh/bun#14105`, módulo `webgl`
V8-direto rejeitado; addons N-API carregam sem reclamar de versão).

## 1.8 Três níveis de addon (acoplamento decrescente)

A doc oficial lista exatamente 3 opções (`addons.html`, confirmado verbatim):

1. **Node-API** (recomendado) — interface C opaca, **ABI-estável**
2. **`nan`** (Native Abstractions for Node.js) — wrapper C++ **sobre o V8 cru**,
   sem garantia de ABI (recompila a cada major)
3. **uso direto** de V8/libuv/internals — acoplamento máximo

`node-addon-api` (≠ `nan`) é um wrapper C++ header-only **sobre o C N-API** —
herda a estabilidade ABI. A doc confirma verbatim:

> "Binaries built with `node-addon-api` will depend on the symbols of the
> Node-API C-based functions exported by Node.js." … "Even though the addon is
> written in C++, it still gets the benefits of the ABI stability provided by
> the C Node-API."

Ex.: `obj["foo"] = String::New(env, "bar")` (node-addon-api) compila para
`napi_create_string_utf8` + `napi_set_named_property`.

## Conclusão do capítulo

- Um `.node` é uma DLL/`.so`/`.dylib` comum; o ponto de entrada é uma **função**
  (`napi_register_module_v1`), não dados estáticos.
- Há dois caminhos de registro: **N-API** (símbolo direto, ABI-estável, sem V8) e
  **legado** (constructor + struct `node_module` acoplada a V8, travado por
  `NODE_MODULE_VERSION`).
- **O único alvo viável para o RTS é N-API.** O caminho legado/V8-direto exige
  reproduzir o ABI binário do V8 — fora de escopo (ver
  [`03-acoplamento-v8-libuv.md`](03-acoplamento-v8-libuv.md)).

## Fontes

- https://nodejs.org/api/addons.html
- https://nodejs.org/api/n-api.html
- https://raw.githubusercontent.com/nodejs/node/main/src/node.h
- https://raw.githubusercontent.com/nodejs/node/main/src/node_api.h
- https://raw.githubusercontent.com/nodejs/node/main/src/node_version.h
- https://raw.githubusercontent.com/nodejs/node/main/src/node_binding.cc
- https://github.com/nodejs/node/blob/main/doc/abi_version_registry.json
- https://github.com/oven-sh/bun/issues/5578 · /issues/23136 · /issues/21432 · /issues/14105
- https://blog.s1h.org/inside-node-loading-native-addons/
