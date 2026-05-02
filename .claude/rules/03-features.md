# Features ativas — capacidades, parallelism, async/Promise/Function

## Capacidades de linguagem ativas (codegen)

- Object/array literals: `{k: v}` e `[1,2,3]` via
  `collections.map_*`/`vec_*`.
- Classes: constructor, method, this, extends, super(args),
  super.method(args), static methods, getters/setters. Instance
  armazena `__rts_class` para dispatch virtual real (override em
  subclasse roteado via comparacao de string sobre o tag de
  runtime).
- Operator overload Rust-style: `a + b` vira `a.add(b)` em
  compile-time quando classe define o metodo
  (`add`/`sub`/`mul`/.../`eq`/`lt`/`bit_*`).
- for...of em arrays; bind herda classe quando array tem anotacao
  `: C[]`.
- try/catch/finally fase 1: slot de erro thread-local, sem unwind
  real (#128 rastreia fase 2).
- String equality: `s1 == s2` compara conteudo via `gc.string_eq`.
- async/await com pipeline Promise-centric (ver secao abaixo).
- Function class — `.call/.apply/.bind/.toString` + `.name/.length`
  + `new Function("body")` via eval em runtime.

## Silent parallelism (Level-1)

O codegen tem 3 passes que reescrevem padroes TS comuns para
chamadas `parallel.*` automaticamente. User nao precisa mencionar
threads/workers:

- **`array_methods_pass`** — detecta `arr.map(fn)`,
  `arr.forEach(fn)`, `arr.reduce(fn, init)` quando `fn` eh Ident de
  user fn → reescreve para `parallel.map/for_each/reduce`. Roda
  primeiro.
- **`reduce_pass`** — detecta padrao classico de acumulador
  (`let s = 0; for (x of arr) s = s + EXPR;` ou `s += EXPR`) e
  reescreve para `parallel.reduce`. So aceita ops associativas
  (+, *).
- **`purity_pass`** — detecta `for...of` cujo corpo so chama
  membros `pure: true` de namespaces e nao tem assignments →
  reescreve para `parallel.for_each`.

Os 3 passes cobrem top-level + body de cada user fn. Counters
compartilhados sem colisao de nomes. 96 fns marcadas `pure: true`
hoje (math, string, num, fmt, path, hash, mem) — base do
reconhecimento.

`parallel.*` aceita arrays literais, em variavel, e retornados de
fn (todos viram Vec<i64> via codegen de array literal). Bridge pra
Buffer e typed arrays eh follow-up.

Spec detalhada: `docs/specs/silent-parallelism.md`.

## async / Promise / Function (Promise-centric, #437)

Subsistema unificado implementado em PRs #428-#437. Design proposto
pelo @drysius: Promise concentra spawn+state; Function eh payload
(fn_ptr + bound_args).

### Pipeline `async function`

```
async function f(a, b) { body }

=> (apos expand_async_functions)

function __async_inner_f(a, b): i64 { body }
function f(a, b): i64 {
    const __args = [a, b];
    return promise.create(__async_inner_f, __args);
}
```

`promise.create(fn_handle, args_vec)` em Rust:
- Aloca PromiseAsync pendente
- Resolve fn (handle Function ou ptr direto via
  `resolve_callback_ptr`)
- `rt.spawn_blocking(move || invoke + settle)`
- Settle automatico: `resolve(retval)` ou `reject(err)` baseado em
  thread-local error slot

`await x` → `promise.wait(x)` (bloqueia em oneshot, propaga
rejection via error slot).

### Function class

- `Entry::Function { fn_ptr, arity, name, bound_this, bound_args,
  is_arrow, source, keep_alive }`
- `__RTS_FN_GL_FUNCTION_REIFY/CALL/APPLY/BIND/NAME/LENGTH/TO_STRING/NEW`
- Trampolim `invoke_n` faz `transmute` para `extern "C" fn(i64...)
  -> i64` por aridade ate 8
- `new Function("a", "body")` via `eval_compile::compile_function`
  → pipeline parser+JIT → JITModule mantido vivo via
  `Arc<Mutex<dyn Any>>` no `keep_alive`

### Integracao Promise+Function

- `promise.then(p, fn)` aceita ptr direto OU handle Function (de
  `bind()` ou `new Function`)
- `resolve_callback_ptr` detecta o tipo via `Entry::Function` lookup
- `userFn.call(...)` em async fn retorna Promise (porque `f` foi
  reescrita pra retornar `promise.create(...)`)

### Limitacoes conscientes

- `this` binding em fn nao-arrow: thisArg ignorado em
  `.call(thisArg)` (RTS user fns sem slot reservado)
- `fn.toString()` em user fn estatica: `"function name() { [native
  code] }"` (source nao preservado)
- `fn.prototype`, `arguments`: nao existem
- `new Function("async ...")`: eval modo native nao permite async
  no top-level
- `invoke_n` aridade > 8: panic (raro em pratica)

Spec detalhada: `docs/specs/async-promise-function.md`.

## Disciplina de regressao zero

Rever a `REGRA OBRIGATORIA: ZERO REGRESSAO ANTES DE MERGE` em
`00-meta.md`. Em projeto com IA acelerando velocidade, eh essa
regra que mantem a suite confiavel ao longo de centenas de PRs.
