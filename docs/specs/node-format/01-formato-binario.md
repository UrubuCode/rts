# 01 — What a `.node` physically is and how Node loads it

> Verified against primary sources: `nodejs.org/api/addons.html`,
> `nodejs.org/api/n-api.html`, and the Node source code
> (`src/node.h`, `src/node_api.h`, `src/node_version.h`, `src/node_binding.cc`).

## 1.1 A `.node` is a native dynamic library — only the extension changes

There is no format container of its own. A `.node` is literally the OS's native
shared library, just with the extension renamed:

| Platform | Actual format | Magic | Note |
|---|---|---|---|
| Linux | ELF shared object (`.so`) | `0x7F 45 4C 46` (`\x7FELF`) | `readelf -h foo.node` works |
| Windows | PE/COFF DLL | `MZ` (`0x4D5A`) → `PE\0\0` | it is literally a `.dll` |
| macOS | Mach-O dylib | `0xFEEDFACF` (64-bit) / `0xCAFEBABE` (fat) | it is literally a `.dylib` |

The official docs confirm verbatim:

> "The filename extension of the compiled addon binary is `.node` (as opposed to
> `.dll` or `.so`). The `require()` function is written to look for files with
> the `.node` file extension and initialize those as dynamically-linked
> libraries."
> — `nodejs.org/api/addons.html`

> "*Addons* are dynamically-linked shared objects that can be loaded via the
> `require()` function as ordinary Node.js modules."

**Implication for RTS:** to *produce* a `.node`, RTS needs to emit the target's
native format (ELF `.so` / PE DLL / Mach-O dylib) as a **shared
object**, not as an executable — reusing the AOT pipeline (`cranelift_object` +
linker), but linking in *shared library* mode with an export table. To
*consume*, the loader is the host runtime's (RTS itself).

## 1.2 The loading flow

```
require('./addon.node')
  → Module._extensions['.node']
  → process.dlopen(module, path.toNamespacedPath(filename))
  → internalBinding (src/node_binding.cc)
  → DLib::Open
      • POSIX:   dlopen(filename, RTLD_LAZY)        [flags configurable via os.constants.dlopen]
      • Windows: uv_dlopen(filename, &lib)
  → DLib::GetSymbolAddress(name)  → dlsym / uv_dlsym
  → invokes the initialization symbol
  → the populated `exports` object becomes the module.exports returned to JS
```

`DLib::Close` calls `dlclose`. The addon **has no `main`**: it only fills in and
returns `exports`.

## 1.3 The TWO entry symbols (confirmed in the code)

`src/node_binding.cc` has two discovery functions. Verified literally
against the source:

**N-API (RTS's target):**
```c
inline napi_addon_register_func GetNapiInitializerCallback(DLib* dlib) {
  const char* name =
      STRINGIFY(NAPI_MODULE_INITIALIZER_BASE) STRINGIFY(NAPI_MODULE_VERSION);
  // NAPI_MODULE_INITIALIZER_BASE = "napi_register_module_v", NAPI_MODULE_VERSION = 1
  // → exact symbol: napi_register_module_v1
```
Signature (extern "C", **no mangling** — any mangling defeats `dlsym`):
`napi_value napi_register_module_v1(napi_env env, napi_value exports)`. Node
resolves it via `dlib->GetSymbolAddress(name)` + `reinterpret_cast` and invokes it via
`napi_module_register_by_symbol(exports, module, context, init, module_api_version)`
— **five arguments** in current Node (the `module_api_version` was added; the
4-arg form only applies to older versions).

**Legacy / raw-V8 (out of RTS's reach):**
```c
inline InitializerCallback GetInitializerCallback(DLib* dlib) {
  const char* name = "node_register_module_v" STRINGIFY(NODE_MODULE_VERSION);
  // → e.g.: node_register_module_v147 (NODE_MODULE_VERSION 147 = Node 26 ABI;
  //   the main branch is at NODE_MAJOR_VERSION 27 in development)
```

The empirical proof that alternative runtimes look for **exactly**
`napi_register_module_v1` is Bun's actual error message (issues
`oven-sh/bun#5578`, `#23136`, `#21432`):

> `TypeError: symbol napi_register_module_v1 not found in native module. Is this a Node API (napi) module?`

## 1.4 Legacy self-registration via constructor (a path RTS ignores)

Legacy addons (`NODE_MODULE`, NAN) self-register via a static constructor
**before** any symbol is looked up. `src/node.h`:

```c
#define NODE_MODULE_X(modname, regfunc, priv, flags)                  \
  static node::node_module _module = {                                \
      NODE_MODULE_VERSION, flags, NULL, __FILE__,                     \
      (node::addon_register_func)(regfunc), NULL,                     \
      NODE_STRINGIFY(modname), priv, NULL };                          \
  NODE_C_CTOR(_register_##modname) { node_module_register(&_module); }
```

`node_module_register` stores the module in a thread-local
(`thread_local node_module* thread_local_modpending;`), which `DLOpen` reads after
the `dlopen`:

```c
CHECK_NULL(thread_local_modpending);
// ...
node_module* mp = thread_local_modpending;
thread_local_modpending = nullptr;
```

> **Verified correction (MSVC detail):** `NODE_C_CTOR` expands to
> `__attribute__((constructor))` on GCC/POSIX, but on MSVC it does **not** use the
> `.CRT$XCU` section — it uses a static struct constructor in an anonymous namespace
> (`struct fn##_ { fn##_(){ fn(); }; } fn##_v_;`), ordinary C++ static
> initialization. (Pointed out by the adversarial verification.)

## 1.5 The `node_module` struct (legacy) is coupled to V8 → unviable in RTS

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
    v8::Local<v8::Object> exports,   // ← V8 types at the boundary
    v8::Local<v8::Value> module, void* priv);
```

The legacy register receives `v8::Local<v8::Object>` — **direct coupling to V8**.
RTS has no V8, so the legacy path is unviable to implement faithfully.

## 1.6 The `napi_module` struct (N-API) is V8-independent → viable

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

`napi_env` and `napi_value` are **opaque pointers** — no V8 type at the boundary.
This is what makes support viable without V8 (see [`02-napi-abi.md`](02-napi-abi.md)).

The `NAPI_MODULE_INIT()` macro exports two symbols with default visibility
(`__declspec(dllexport)` on Windows / `visibility("default")` on POSIX):
- `napi_register_module_v1(napi_env, napi_value) -> napi_value` (required)
- `node_api_module_get_api_version_v1(void) -> int32_t` (version negotiation)

## 1.7 `NODE_MODULE_VERSION` — ABI lock of the **legacy** path only

`src/node_version.h`: `#define NODE_MODULE_VERSION 147`. According to the
official `doc/abi_version_registry.json`, **147 corresponds to Node 26** (variant
v8_14.6); the value appears in the `main` branch's `node_version.h` while
`NODE_MAJOR_VERSION` is at 27 (development), but the ABI was registered for
Node 26. Node refuses legacy addons compiled against a different ABI:

```
"The module '%s' was compiled against a different Node.js version using
NODE_MODULE_VERSION %d. This version of Node.js requires NODE_MODULE_VERSION %d."
```

Confirmed values: Node 16=93, 18=108, 20=115, 21=120, 22=127, 23=131, 24=137,
**26=147**. Canonical registry: `doc/abi_version_registry.json`. Exposed in JS as
`process.versions.modules`.

**Decisive point:** **N-API addons skip this check** (the `napi_module` struct
carries its own fixed `NAPI_MODULE_VERSION` and the verification is different).
Confirmed empirically: Bun reports `NODE_MODULE_VERSION 127` and the check
only bites **non-N-API** addons (issue `oven-sh/bun#14105`, the `webgl` module,
V8-direct, rejected; N-API addons load without complaining about version).

## 1.8 Three addon levels (decreasing coupling)

The official docs list exactly 3 options (`addons.html`, confirmed verbatim):

1. **Node-API** (recommended) — opaque C interface, **ABI-stable**
2. **`nan`** (Native Abstractions for Node.js) — C++ wrapper **over raw V8**,
   no ABI guarantee (recompiles every major)
3. **direct use** of V8/libuv/internals — maximum coupling

`node-addon-api` (≠ `nan`) is a header-only C++ wrapper **over the C N-API** —
it inherits the ABI stability. The docs confirm verbatim:

> "Binaries built with `node-addon-api` will depend on the symbols of the
> Node-API C-based functions exported by Node.js." … "Even though the addon is
> written in C++, it still gets the benefits of the ABI stability provided by
> the C Node-API."

E.g.: `obj["foo"] = String::New(env, "bar")` (node-addon-api) compiles to
`napi_create_string_utf8` + `napi_set_named_property`.

## Chapter conclusion

- A `.node` is an ordinary DLL/`.so`/`.dylib`; the entry point is a **function**
  (`napi_register_module_v1`), not static data.
- There are two registration paths: **N-API** (direct symbol, ABI-stable, no V8) and
  **legacy** (constructor + `node_module` struct coupled to V8, locked by
  `NODE_MODULE_VERSION`).
- **The only viable target for RTS is N-API.** The legacy/V8-direct path requires
  reproducing V8's binary ABI — out of scope (see
  [`03-acoplamento-v8-libuv.md`](03-acoplamento-v8-libuv.md)).

## Sources

- https://nodejs.org/api/addons.html
- https://nodejs.org/api/n-api.html
- https://raw.githubusercontent.com/nodejs/node/main/src/node.h
- https://raw.githubusercontent.com/nodejs/node/main/src/node_api.h
- https://raw.githubusercontent.com/nodejs/node/main/src/node_version.h
- https://raw.githubusercontent.com/nodejs/node/main/src/node_binding.cc
- https://github.com/nodejs/node/blob/main/doc/abi_version_registry.json
- https://github.com/oven-sh/bun/issues/5578 · /issues/23136 · /issues/21432 · /issues/14105
- https://blog.s1h.org/inside-node-loading-native-addons/
