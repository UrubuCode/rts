# 02 — The N-API / Node-API ABI (the addons' stable C interface)

> Verified against `nodejs.org/api/n-api.html` and the headers
> `src/js_native_api_types.h`, `src/js_native_api_v8.h`, `src/node_api.h`.
> N-API is the **only** surface a non-V8 runtime (RTS, Bun) can
> implement — see [`01-formato-binario.md`](01-formato-binario.md).

## 2.1 `napi_value` — opaque pointer

```c
typedef struct napi_value__* napi_value;   // incomplete struct, never defined
```

Official docs (confirmed verbatim):
> "All JavaScript values are abstracted behind an opaque type named
> `napi_value`. This is an opaque pointer that is used to represent a JavaScript
> value."

The addon **never** dereferences `napi_value` — it only passes it back to the
`napi_*` functions. In real Node, `napi_value` is binarily identical to a
`v8::Local<v8::Value>` (8 bytes):

```c
// src/js_native_api_v8.h
static_assert(sizeof(v8::Local<v8::Value>) == sizeof(napi_value), ...);
inline napi_value JsValueFromV8LocalValue(v8::Local<v8::Value> local) {
  return reinterpret_cast<napi_value>(*local);
}
```

**Key point for RTS:** since the addon treats `napi_value` as opaque, RTS is
**free to choose what it encapsulates** — a `u64` handle from the `HandleTable`
or a pointer to an internal `RuntimeValue`. This is what makes support viable
without V8 (Bun maps it to `JSC::JSValue`, likewise 8 bytes).

## 2.2 `napi_env` — opaque context passed to every function

```c
typedef struct napi_env__* napi_env;
```

In Node it carries `v8::Isolate*` + `v8impl::Persistent<v8::Context>`. ABI rules:
- the **same** `napi_env` of the initial function must be passed on to every
  nested N-API call;
- it must **not** be cached for general reuse nor shared across Worker
  threads;
- it becomes invalid when the addon instance is unloaded.

**In RTS:** `napi_env` = pointer to an `RtsNapiEnv` containing: the `HandleTable`,
the handle-scope stack, the pending-exception slot, instance-data, and the pointer
to the event loop (tokio bridge). One `napi_env` **per addon instance**.

## 2.3 Uniform convention: returns `napi_status`, writes to an out-param

Every N-API function follows:
```c
napi_status napi_create_double(napi_env env, double value, napi_value* result);
```

`napi_status` (ABI-stable enum, ~24 values, fixed order): `napi_ok=0`,
`napi_invalid_arg`, `napi_object_expected`, `napi_string_expected`,
`napi_number_expected`, `napi_function_expected`, `napi_pending_exception`,
`napi_generic_failure`, `napi_escape_called_twice`, `napi_handle_scope_mismatch`,
`napi_bigint_expected`, `napi_date_expected`, `napi_arraybuffer_expected`,
`napi_cannot_run_js`, etc. Detail via `napi_get_last_error_info`.

**In RTS:** each `extern "C" __rts_napi_*` shim returns `i32` (status) and receives
output pointers — **aligned with RTS's typed ABI model**. The enum order is
ABI-stable and cannot be reordered.

## 2.4 Callbacks: how JS calls a native function

```c
typedef napi_value (NAPI_CDECL* napi_callback)(napi_env env, napi_callback_info info);
napi_status napi_get_cb_info(napi_env env, napi_callback_info cbinfo,
                             size_t* argc, napi_value* argv,
                             napi_value* this_arg, void** data);
```

`argc` is in/out (capacity on entry, actual count on exit); `this_arg` receives the
receiver; `data` receives the `void*` registered in `napi_create_function`.

**Calling-convention divergence:** RTS uses `CallConv::Tail` for user fns,
but `napi_callback` requires native `extern "C"`/CDECL. The
RTS→addon→RTS trampoline needs to **switch conventions** — there is already precedent in RTS
(`invoke_all_i64` with win64 asm).

## 2.5 The core function set (the "80/20" almost every addon uses)

The total surface is **~150-160 functions** (`napi_*`/`node_api_*`) — the count
varies by source: `node-api-headers` v10 lists ~111 (89 `js_native_api` + 22
`node_api`), Node's headers have ~161 `NAPI_EXTERN` declarations, Bun tracks
"156/156" (issue #158) and Deno's `symbol_exports.json` lists 163 (some of them
from libuv/loop). The core of a typical addon is ~30-40:

**Registration:** `napi_register_module_v1`, `napi_module` struct.

**Value creation** (`env`, native data…, `out napi_value*`):
`napi_create_double/int32/uint32/int64/bigint_*`,
`napi_create_string_utf8` (length can be `NAPI_AUTO_LENGTH`),
`napi_create_object`, `napi_create_array`, `napi_create_array_with_length`,
`napi_get_boolean`, `napi_get_undefined`, `napi_get_null`, `napi_get_global`.

**Extraction** (`env`, `napi_value`, `out C-type*`):
`napi_get_value_double/int32/uint32/int64/bool`,
`napi_get_value_string_utf8(env, val, char* buf, size_t bufsize, size_t* result)`
— **two-pass protocol**: `buf=NULL` measures the length, then copies.
Implementing this faithfully is critical (addons pre-allocate buffers).

**Properties:**
`napi_get_property`/`napi_set_property` (`napi_value` key),
`napi_get_named_property`/`napi_set_named_property` (C string),
`napi_has_property`, `napi_delete_property`,
`napi_get_element`/`napi_set_element` (`uint32_t` index),
`napi_is_array`, `napi_define_properties(…, const napi_property_descriptor*)`.

**Functions and calls:**
`napi_create_function(env, name, len, napi_callback, void* data, out)`,
`napi_call_function(env, recv, func, argc, argv, out)`,
`napi_new_instance`, `napi_define_class`.

**Types and coercion:**
`napi_typeof` → `napi_valuetype` { `napi_undefined`, `napi_null`,
`napi_boolean`, `napi_number`, `napi_string`, `napi_symbol`, `napi_object`,
`napi_function`, `napi_external`, `napi_bigint` };
`napi_coerce_to_string/number/bool/object`;
`napi_create_external(env, void* data, finalize_cb, hint, out)` — wraps a
raw native pointer visible to JS only as a handle.

**Errors/exceptions:**
`napi_throw`, `napi_throw_error/type_error/range_error`,
`napi_is_exception_pending`, `napi_get_and_clear_last_exception`.

## 2.6 Handle scopes — lifetime of `napi_value`s

```c
napi_open_handle_scope(env, napi_handle_scope*);
napi_close_handle_scope(env, napi_handle_scope);
napi_open_escapable_handle_scope(env, napi_escapable_handle_scope*);
napi_close_escapable_handle_scope(env, scope);
napi_escape_handle(env, scope, napi_value escapable, napi_value* result);
```

Docs (confirmed verbatim):
> "Closing the scope can indicate to the GC that all `napi_value`s created
> during the lifetime of the handle scope are no longer referenced from the
> current stack frame."

Rules: scopes close in reverse order; a *default scope* already exists on entry
to a native method; `napi_escape_handle` may be called only **once** per
scope (otherwise `napi_escape_called_twice`).

**Fundamental GC divergence (not a blocker):** in V8 (moving GC) the handle
scope is *mandatory*. In RTS (**non-moving** mark+sweep, Cranelift stack maps) the
semantics can be simplified — **but RTS MUST implement all 5 functions**
because real addons call them in loops to avoid accumulating handles. Minimum viable:
each handle scope is a vector of handles registered as an **extra root** in
RTS's GC scanner; closing the scope unregisters it; `napi_escape_handle` promotes
a handle to the parent scope. (This is exactly what Bun does: a scannable
slot array.)

## 2.7 References and finalizers — GC integration

```c
napi_create_reference(env, value, uint32_t initial_refcount, napi_ref*);  // 0=weak, >0=strong
napi_reference_ref/unref(env, ref, uint32_t* result);
napi_get_reference_value(env, ref, napi_value*);  // null if already collected (weak)
napi_delete_reference(env, ref);

napi_wrap(env, js_object, void* native, napi_finalize cb, void* hint, napi_ref*);
napi_unwrap(env, js_object, void** result);
napi_add_finalizer(env, js_object, void* native, napi_finalize cb, void* hint, napi_ref*);

typedef void (NAPI_CDECL* napi_finalize)(napi_env env, void* data, void* hint);
```

`napi_ref` keeps values alive **beyond** the handle scope (e.g.: a constructor
kept between calls). `napi_wrap` associates a native C++ object with a JS
object with a finalizer on collection — the foundation of almost every addon exposing a
native resource (DB handle, socket).

**In RTS:** a ref table with refcount that counts as a **GC root** when strong
(>0) and does **not** count when weak (0); `sweep_all_shards()` must, upon freeing
an `Entry` with an associated N-API finalizer, **enqueue** the
`napi_finalize` call — **outside** the mark phase ("second-pass" timing: calling
the engine during the weak callback is unsafe). This ties to RTS issue **#217**
(WeakMap/WeakSet currently with strong semantics).

## 2.8 Promises and async

```c
napi_create_promise(env, napi_deferred* deferred, napi_value* promise);
napi_resolve_deferred(env, deferred, napi_value resolution);
napi_reject_deferred(env, deferred, napi_value rejection);
```

**In RTS:** maps almost 1:1 to RTS's `promise.create`/`PromiseAsync`
subsystem (#437) — the `deferred` is the resolution side RTS already models. An
integration point that's nearly ready.

## 2.9 Threadsafe functions and async work (the hard step)

```c
napi_create_async_work / napi_queue_async_work / napi_cancel_async_work
napi_create_threadsafe_function / napi_call_threadsafe_function
napi_acquire/release/ref/unref_threadsafe_function
napi_get_uv_event_loop(env, uv_loop_t**)
```

They assume the **libuv event loop**. Detailed in
[`03-acoplamento-v8-libuv.md`](03-acoplamento-v8-libuv.md) and
[`05-divergencias-rts.md`](05-divergencias-rts.md) (divergence 4) — this is the area
of highest long-tail incompatibility risk (even Bun has gaps here).

## 2.10 ABI stability and versioning

Official docs (confirmed verbatim):
> "Node-API … is independent from the underlying JavaScript runtime (for
> example, V8) … This API will be Application Binary Interface (ABI) stable
> across versions of Node.js."

**Cumulative** versioning (each version = backwards-compatible):
N-API 8 → Node 12.22+/14.17+/16+; 9 → 18.17+/20.3+/21+; 10 → 22.14+/23.6+.
`#define NAPI_VERSION X` before the include pins the version in the addon.
`napi_get_version(env, uint32_t*)` returns the N-API version supported at runtime.

**In RTS:** implement an entire target version (e.g.: start at N-API 8 or 9);
`napi_get_version` announces the implemented level.

## Chapter conclusion

- Opaque `napi_value`/`napi_env` = RTS controls 100% of the representation → support
  without V8 is viable (Bun is the existence proof on JSC).
- The `napi_status` + out-params convention matches RTS's typed `extern "C"`
  ABI.
- The GC integration points (handle scopes, refs, finalizers) are real but
  known — they map to RTS's `HandleTable`/mark+sweep.
- async/threadsafe is the hard step (libuv event loop vs tokio).

## Sources

- https://nodejs.org/api/n-api.html
- https://raw.githubusercontent.com/nodejs/node/main/src/js_native_api_types.h
- https://raw.githubusercontent.com/nodejs/node/main/src/js_native_api_v8.h
- https://github.com/nodejs/node-addon-api/blob/main/doc/handle_scope.md
- https://bun.com/blog/how-bun-supports-v8-apis-without-using-v8-part-1
- https://nodejs.org/en/learn/modules/abi-stability
