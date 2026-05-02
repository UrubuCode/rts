# Sistema async / Promise / Function (#359, #411)

Subsistema unificado de programacao assincrona e funcoes de primeira
classe no RTS. Implementado via PRs #428-#437. Design Promise-centric
proposto pelo @drysius.

## Visao geral

Tres camadas que compartilham estado:

1. **Function class** (`src/namespaces/globals/function/`) — payload
   invocavel: `fn_ptr` + `bound_args` + `name` + lifetime guard de
   JITModule (pra `new Function`).
2. **PromiseAsync** (`src/namespaces/gc/promise_slot.rs`) — state
   machine pending/fulfilled/rejected + waiters via tokio oneshot.
3. **Bridge** (`promise.create`, `resolve_callback_ptr`) — Promise eh
   o ponto de concentracao de spawn+invoke+settle. Function eh
   passada como handle e invocada dentro da Promise task.

## Pipeline

### `async function f(a, b) { body }` — desugar pelo
`expand_async_functions` (`src/codegen/lower/func.rs`):

```
function __async_inner_f(a, b): i64 { body }     // body original
function f(a, b): i64 {
    const __args = [a, b];
    return promise.create(__async_inner_f, __args);
}
```

`is_async = false` apos o pass — fns sao tratadas como sync dali pra
frente.

### `await x` — pelo `expand_await_exprs` pass:

```
await x  =>  promise.wait(x)
```

`promise.wait(handle)` bloqueia a thread atual no `oneshot::Receiver`
do PromiseSlot ate settle. Se rejected, seta `error_slot` thread-local
pro `try/catch` propagar (F5 #416).

### `promise.create(fn_handle, args_vec)` — entrypoint Rust

```rust
let (fn_ptr, bound, _, _) = resolve_callback_ptr(fn_handle);
rt.spawn_blocking(move || {
    let r = unsafe { invoke_callback_full(fn_ptr, &all_args) };
    let err = error::__RTS_FN_RT_ERROR_GET();
    if err != 0 {
        promise_slot::reject(&result, err);
    } else {
        promise_slot::resolve(&result, r);
    }
});
```

- `resolve_callback_ptr` aceita handle Function ou ptr extern "C"
  direto (compat legado).
- `invoke_callback_full` faz `transmute` para `extern "C" fn(i64...)
  -> i64` por aridade (limite atual 8).
- Settle automatico baseado em error slot thread-local.

### `Function.call/apply/bind/toString` — codegen

Quando `<userFn>.method(...)` ou `<varHandle>.method(...)`:

1. **Reify**: `__RTS_FN_GL_FUNCTION_REIFY(fn_ptr, arity, name, is_arrow)`
   aloca `Entry::Function`.
2. **Empacota args** em Vec via `collections.vec_new` + `vec_push`.
3. **Despacha** pra symbol correspondente:
   - `__RTS_FN_GL_FUNCTION_CALL(handle, this, args_vec)`
   - `__RTS_FN_GL_FUNCTION_APPLY(handle, this, args_vec)`
   - `__RTS_FN_GL_FUNCTION_BIND(handle, this, args_vec)` — retorna novo handle

**Async fn passa pelo mesmo path** porque `expand_async_functions`
reescreve `f` para retornar `promise.create(...)` — o `.call(...)` em
async fn retorna **Promise**, nao i64. Caller usa `await`.

### `new Function("a", "b", "body")` — codegen + eval

1. Codegen empacota args[0..n-1] em CSV via `gc.string_concat`.
2. `__RTS_FN_GL_FUNCTION_NEW(params_csv, body)` chama
   `eval_compile::compile_function`:
   - Constroi source `function __rts_eval_fn(p1: i64, ...): i64 { body }`
   - Truque: helper `__rts_eval_keep_alive(__rts_eval_fn)` forca
     address-taken → callconv C compativel com transmute.
   - `parse_source_with_mode` → `compile_program_to_jit`.
   - Retorna fn_ptr + JITModule (Arc<Mutex>) como keep_alive.
3. Aloca `Entry::Function` com source preservado.

JITModule mantido vivo enquanto o handle Function existir. Mutex
existe so' por Sync — nunca destravado em runtime hot path.

## Limites de aridade

- `invoke_n` / `invoke_callback_full`: 0-8 args.
- `new Function`: arity > 8 retorna erro de compilacao.
- Async fn declarada com >8 params: codegen ainda gera, mas o
  trampolim retornaria 0 silenciosamente (nao testado, evitar).

## Resolucao de bugs antigos

| Bug | Como ficou |
|---|---|
| `promise.then(p, fn_handle)` SIGSEGV | `resolve_callback_ptr` extrai fn_ptr de Entry::Function antes de transmutar |
| `add.bind(0).length == -1` | `local_class_ty.insert(name, "Function")` em decls.rs pra `.bind(...)` callee |
| `userFn.name == ""` em template literal | path explicito `<userFn>.name/.length` em members.rs antes de cair no fallback handle_len |
| Aliasing `const b = a` perdia tipo | propagacao de `local_class_ty` em decls.rs |

## Limitacoes conscientes vs Node

- `this` binding em fn declarations nao-arrow: `thisArg` ignorado em
  `.call(thisArg)`. RTS user fns nao tem slot reservado pra this
  implicito. Refator futuro junto com #359 calls.rs.
- `fn.toString()` em user fn estatica: retorna `"function name() {
  [native code] }"`. Source nao preservado (so' fns de `new Function`
  tem source).
- `fn.prototype`: nao existe (RTS separa classes de functions).
- `arguments`: nao existe (use rest params).
- `new Function("async function...")`: eval modo native nao permite
  async no top-level do body. Tem que envolver em fn sync que retorna
  o resultado de outra Promise.

## Como adicionar uma fn async

```ts
// Use async function — desugar automatico:
async function fetchData(url: string): i64 {
    // body sync; await dentro funciona
}

// OU use promise.create direto:
function compute(x: i64, y: i64): i64 { return x + y; }
const p = promise.create(compute, [3, 4]);
const r = await p;  // 7
```

Ambos resultam no mesmo IR. `async function` eh acucar.

## Ver tambem

- `src/namespaces/promise/abi.rs` — ABI completa de Promise
- `src/namespaces/globals/function/abi.rs` — Function class spec
- `src/codegen/lower/func.rs::expand_async_functions` — pass de async
- `src/namespaces/gc/promise_slot.rs` — state machine
- `examples/async_promise_function_demo.ts` — demo executavel
- Testes: `tests/promise_*`, `tests/await_*`, `tests/async_*`,
  `tests/function_global.test.ts`, `tests/promise_with_function.test.ts`
- PRs: #428 (combinators), #429 (rejection), #430 (then/catch/finally),
  #431 (top-level await), #437 (Function class + integracao)
