# 02 — A ABI N-API / Node-API (a interface C estável dos addons)

> Verificado contra `nodejs.org/api/n-api.html` e os headers
> `src/js_native_api_types.h`, `src/js_native_api_v8.h`, `src/node_api.h`.
> A N-API é a **única** superfície que um runtime não-V8 (RTS, Bun) pode
> implementar — ver [`01-formato-binario.md`](01-formato-binario.md).

## 2.1 `napi_value` — ponteiro opaco

```c
typedef struct napi_value__* napi_value;   // struct incompleto, nunca definido
```

Doc oficial (confirmado verbatim):
> "All JavaScript values are abstracted behind an opaque type named
> `napi_value`. This is an opaque pointer that is used to represent a JavaScript
> value."

O addon **nunca** dereferencia `napi_value` — só o passa de volta às funções
`napi_*`. No Node real, `napi_value` é binariamente igual a um
`v8::Local<v8::Value>` (8 bytes):

```c
// src/js_native_api_v8.h
static_assert(sizeof(v8::Local<v8::Value>) == sizeof(napi_value), ...);
inline napi_value JsValueFromV8LocalValue(v8::Local<v8::Value> local) {
  return reinterpret_cast<napi_value>(*local);
}
```

**Ponto-chave para o RTS:** como o addon trata `napi_value` como opaco, o RTS é
**livre para escolher o que ele encapsula** — um handle `u64` da `HandleTable`
ou um ponteiro para um `RuntimeValue` interno. É isto que torna o suporte viável
sem V8 (o Bun mapeia para `JSC::JSValue`, igualmente 8 bytes).

## 2.2 `napi_env` — contexto opaco passado a toda função

```c
typedef struct napi_env__* napi_env;
```

No Node carrega `v8::Isolate*` + `v8impl::Persistent<v8::Context>`. Regras de ABI:
- o **mesmo** `napi_env` da função inicial deve ser repassado a toda chamada
  N-API aninhada;
- **não** pode ser cacheado para reuso geral nem compartilhado entre Worker
  threads;
- torna-se inválido quando a instância do addon é descarregada.

**No RTS:** `napi_env` = ponteiro para uma `RtsNapiEnv` contendo: a `HandleTable`,
a pilha de handle scopes, o slot de exceção pendente, instance-data, e o ponteiro
para o event loop (ponte tokio). Um `napi_env` **por instância de addon**.

## 2.3 Convenção uniforme: retorna `napi_status`, escreve em out-param

Toda função N-API segue:
```c
napi_status napi_create_double(napi_env env, double value, napi_value* result);
```

`napi_status` (enum ABI-estável, ~24 valores, ordem fixa): `napi_ok=0`,
`napi_invalid_arg`, `napi_object_expected`, `napi_string_expected`,
`napi_number_expected`, `napi_function_expected`, `napi_pending_exception`,
`napi_generic_failure`, `napi_escape_called_twice`, `napi_handle_scope_mismatch`,
`napi_bigint_expected`, `napi_date_expected`, `napi_arraybuffer_expected`,
`napi_cannot_run_js`, etc. Detalhe via `napi_get_last_error_info`.

**No RTS:** cada shim `extern "C" __rts_napi_*` retorna `i32` (status) e recebe
ponteiros de saída — **alinhado ao modelo ABI tipado do RTS**. A ordem do enum é
ABI-estável e não pode ser reordenada.

## 2.4 Callbacks: como JS chama uma função nativa

```c
typedef napi_value (NAPI_CDECL* napi_callback)(napi_env env, napi_callback_info info);
napi_status napi_get_cb_info(napi_env env, napi_callback_info cbinfo,
                             size_t* argc, napi_value* argv,
                             napi_value* this_arg, void** data);
```

`argc` é in/out (capacidade na entrada, nº real na saída); `this_arg` recebe o
receiver; `data` recebe o `void*` registrado em `napi_create_function`.

**Divergência de convenção de chamada:** o RTS usa `CallConv::Tail` para fns de
usuário, mas `napi_callback` exige `extern "C"`/CDECL nativo. O trampolim
RTS→addon→RTS precisa **trocar de convenção** — já há precedente no RTS
(`invoke_all_i64` com asm win64).

## 2.5 O núcleo de funções (o "80/20" que quase todo addon usa)

A superfície total é de **~150-160 funções** (`napi_*`/`node_api_*`) — a contagem
varia por fonte: `node-api-headers` v10 lista ~111 (89 `js_native_api` + 22
`node_api`), os headers do Node têm ~161 declarações `NAPI_EXTERN`, o Bun rastreia
"156/156" (issue #158) e o `symbol_exports.json` do Deno lista 163 (parte deles
de libuv/loop). O núcleo de um addon típico são ~30-40:

**Registro:** `napi_register_module_v1`, struct `napi_module`.

**Criação de valores** (`env`, dados nativos…, `out napi_value*`):
`napi_create_double/int32/uint32/int64/bigint_*`,
`napi_create_string_utf8` (length pode ser `NAPI_AUTO_LENGTH`),
`napi_create_object`, `napi_create_array`, `napi_create_array_with_length`,
`napi_get_boolean`, `napi_get_undefined`, `napi_get_null`, `napi_get_global`.

**Extração** (`env`, `napi_value`, `out C-type*`):
`napi_get_value_double/int32/uint32/int64/bool`,
`napi_get_value_string_utf8(env, val, char* buf, size_t bufsize, size_t* result)`
— **protocolo de duas passagens**: `buf=NULL` mede o comprimento, depois copia.
Implementar isso fielmente é crítico (addons pré-alocam buffers).

**Propriedades:**
`napi_get_property`/`napi_set_property` (chave `napi_value`),
`napi_get_named_property`/`napi_set_named_property` (string C),
`napi_has_property`, `napi_delete_property`,
`napi_get_element`/`napi_set_element` (índice `uint32_t`),
`napi_is_array`, `napi_define_properties(…, const napi_property_descriptor*)`.

**Funções e chamadas:**
`napi_create_function(env, name, len, napi_callback, void* data, out)`,
`napi_call_function(env, recv, func, argc, argv, out)`,
`napi_new_instance`, `napi_define_class`.

**Tipos e coerção:**
`napi_typeof` → `napi_valuetype` { `napi_undefined`, `napi_null`,
`napi_boolean`, `napi_number`, `napi_string`, `napi_symbol`, `napi_object`,
`napi_function`, `napi_external`, `napi_bigint` };
`napi_coerce_to_string/number/bool/object`;
`napi_create_external(env, void* data, finalize_cb, hint, out)` — embrulha um
ponteiro nativo cru visível ao JS só como handle.

**Erros/exceções:**
`napi_throw`, `napi_throw_error/type_error/range_error`,
`napi_is_exception_pending`, `napi_get_and_clear_last_exception`.

## 2.6 Handle scopes — lifetime dos `napi_value`

```c
napi_open_handle_scope(env, napi_handle_scope*);
napi_close_handle_scope(env, napi_handle_scope);
napi_open_escapable_handle_scope(env, napi_escapable_handle_scope*);
napi_close_escapable_handle_scope(env, scope);
napi_escape_handle(env, scope, napi_value escapable, napi_value* result);
```

Doc (confirmada verbatim):
> "Closing the scope can indicate to the GC that all `napi_value`s created
> during the lifetime of the handle scope are no longer referenced from the
> current stack frame."

Regras: scopes fecham em ordem inversa; já existe um *default scope* na entrada
de um método nativo; `napi_escape_handle` só pode ser chamado **uma vez** por
scope (senão `napi_escape_called_twice`).

**Divergência fundamental de GC (não bloqueador):** no V8 (GC móvel) o handle
scope é *obrigatório*. No RTS (mark+sweep **não-móvel**, stack maps Cranelift) a
semântica pode ser simplificada — **mas o RTS DEVE implementar as 5 funções**
porque addons reais as chamam em loops para não acumular handles. Mínimo viável:
cada handle scope é um vetor de handles registrado como **root extra** no
scanner do GC do RTS; fechar o scope desregistra; `napi_escape_handle` promove
um handle ao scope pai. (É exatamente o que o Bun faz: array de slots
escaneável.)

## 2.7 Referências e finalizers — integração com o GC

```c
napi_create_reference(env, value, uint32_t initial_refcount, napi_ref*);  // 0=weak, >0=strong
napi_reference_ref/unref(env, ref, uint32_t* result);
napi_get_reference_value(env, ref, napi_value*);  // null se já coletado (weak)
napi_delete_reference(env, ref);

napi_wrap(env, js_object, void* native, napi_finalize cb, void* hint, napi_ref*);
napi_unwrap(env, js_object, void** result);
napi_add_finalizer(env, js_object, void* native, napi_finalize cb, void* hint, napi_ref*);

typedef void (NAPI_CDECL* napi_finalize)(napi_env env, void* data, void* hint);
```

`napi_ref` mantém valores vivos **além** do handle scope (ex.: um constructor
guardado entre chamadas). `napi_wrap` associa um objeto nativo C++ a um objeto
JS com finalizer na coleta — base de quase todo addon que expõe um recurso
nativo (DB handle, socket).

**No RTS:** tabela de refs com refcount que conta como **GC root** quando strong
(>0) e **não** conta quando weak (0); o `sweep_all_shards()` precisa, ao liberar
um `Entry` com finalizer N-API associado, **enfileirar** a chamada do
`napi_finalize` — **fora** da fase de marcação (timing "second-pass": chamar o
motor durante o weak callback é inseguro). Liga-se à issue **#217** do RTS
(WeakMap/WeakSet hoje com semântica forte).

## 2.8 Promises e async

```c
napi_create_promise(env, napi_deferred* deferred, napi_value* promise);
napi_resolve_deferred(env, deferred, napi_value resolution);
napi_reject_deferred(env, deferred, napi_value rejection);
```

**No RTS:** mapeia quase 1:1 ao subsistema `promise.create`/`PromiseAsync` do
RTS (#437) — o `deferred` é o lado de resolução que o RTS já modela. Ponto de
integração quase pronto.

## 2.9 Threadsafe functions e async work (o degrau difícil)

```c
napi_create_async_work / napi_queue_async_work / napi_cancel_async_work
napi_create_threadsafe_function / napi_call_threadsafe_function
napi_acquire/release/ref/unref_threadsafe_function
napi_get_uv_event_loop(env, uv_loop_t**)
```

Assumem o **event loop libuv**. Detalhado em
[`03-acoplamento-v8-libuv.md`](03-acoplamento-v8-libuv.md) e
[`05-divergencias-rts.md`](05-divergencias-rts.md) (divergência 4) — é a área de
maior risco de incompatibilidade de cauda longa (até o Bun tem gaps aqui).

## 2.10 Estabilidade ABI e versionamento

Doc oficial (confirmada verbatim):
> "Node-API … is independent from the underlying JavaScript runtime (for
> example, V8) … This API will be Application Binary Interface (ABI) stable
> across versions of Node.js."

Versionamento **cumulativo** (cada versão = retrocompatível):
N-API 8 → Node 12.22+/14.17+/16+; 9 → 18.17+/20.3+/21+; 10 → 22.14+/23.6+.
`#define NAPI_VERSION X` antes do include "baka" a versão no addon.
`napi_get_version(env, uint32_t*)` retorna a versão N-API suportada em runtime.

**No RTS:** implementar uma versão alvo inteira (ex.: começar em N-API 8 ou 9);
`napi_get_version` anuncia o nível implementado.

## Conclusão do capítulo

- `napi_value`/`napi_env` opacos = o RTS controla 100% da representação → suporte
  sem V8 é viável (Bun é a prova de existência sobre JSC).
- A convenção `napi_status` + out-params casa com a ABI tipada `extern "C"` do
  RTS.
- Os pontos de integração de GC (handle scopes, refs, finalizers) são reais mas
  conhecidos — mapeiam à `HandleTable`/mark+sweep do RTS.
- async/threadsafe é o degrau difícil (event loop libuv vs tokio).

## Fontes

- https://nodejs.org/api/n-api.html
- https://raw.githubusercontent.com/nodejs/node/main/src/js_native_api_types.h
- https://raw.githubusercontent.com/nodejs/node/main/src/js_native_api_v8.h
- https://github.com/nodejs/node-addon-api/blob/main/doc/handle_scope.md
- https://bun.com/blog/how-bun-supports-v8-apis-without-using-v8-part-1
- https://nodejs.org/en/learn/modules/abi-stability
