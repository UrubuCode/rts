# Features ativas — capacidades, parallelism, async/Promise/Function

## Pipeline HIR → MIR → Cranelift (routing hibrido, default ON)

Fase 3 do `RTS_REFACTOR.md` entregue: o crate `rts-mir` esta ativo
por default (commits f7b924b/23dd4b7). Pipeline atual:

```
TS → SWC → AST → HIR (rts-hir) → MIR (rts-mir) → inline (fixed-point) → optimize (fold→fma→cse→dce) → mir_codegen → Cranelift
                                              ↘ AST autoritativo (fallback)
```

Fase 4 em progresso (5/8 entregues): atomics no MIR (4.1), inline +
integracao + fixed-point (4.2/4.3/4.7), CSE intra-bloco (4.5), FMA
fusion `a*b+c → Fma` (4.8), smoke e2e + arr[i]=v (4.4/4.6). Restam
escape analysis, SIMD e narrow storage real.

Cada user fn tenta o caminho HIR→MIR→Cranelift; se bate em construct
ainda nao modelado (member em `this`/objetos, classes, async/await,
address-taken fns, string em params/ret de user fn), cai automatico
no codegen AST. Bail e' silencioso e nao quebra semantica.

Variavel `RTS_USE_MIR`:

| Valor | Comportamento |
|---|---|
| unset / `1` / `on` / `all` | MIR ON (default) |
| `0` / `off` / `none` | AST only |
| `fn1,fn2,...` | MIR so' pras fns listadas |

**Capacidades MIR:** literais (int/float/bool/string/null/undefined),
aritmetica inteira/float, bitwise, shifts, comparacoes, casts; control
flow completo (if/else, while, do-while, for classico, break/continue
com loop_stack, ternary→Select, switch via br_table, throw→Trap,
try/finally fase 1); mutacao SSA via block params (`let i = i + 1` em
loops); cross-fn calls (CallUser) + recursao mutua; auto-recursao bail
(TCO so' no AST); extern calls para namespaces RTS via `CallExtern` +
resolver via SPECS; intrinsic inlining (math.sqrt, abs/min/max
f64/i64); string literais (`StrLit` data segment + StrPtr ptr+len);
namespace constants (math.PI, math.E); arrays simples via
`collections.vec_*`; GC stack maps automaticos via
`declare_value_needs_stack_map`.

**Metricas atuais:** 438 user fns reais da suite TS rodam pelo MIR.
`cargo test --release --lib` 12/12; `rts-hir` 27/27; `rts-mir` **59/59**
(+8 vs Fase 3 final, cobrindo inline/CSE/FMA); `rts-codegen --lib
mir_codegen` **61/61** (+8 vs Fase 3); `rts.exe test` 622/632 (mesmas
10 falhas pre-existentes do AST).

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
- Destructuring (#210): array/objeto, defaults, rest, aninhado,
  em params de fn/arrow, em for-of, em catch, alias `{a: b}`.
- Builtins JS expandidos (epic #226 em progresso): Array
  (indexOf/lastIndexOf/includes com fromIndex, reverse, shift/
  unshift, slice, concat, fill, flat/flatMap, splice, findLast/
  findLastIndex, reduceRight, copyWithin, sort com strings,
  values/keys/entries, toSorted/toReversed/toSpliced/with,
  Array.from(length) e Array.from(arr)); Object (entries, assign,
  freeze, fromEntries, seal, isFrozen, isSealed, getPrototypeOf,
  defineProperty); Math (sign, hypot, expm1, log1p, fround,
  sinh/cosh/tanh/asinh/acosh/atanh, imul, clz32 + SQRT2/SQRT1_2/
  LN2/LN10/LOG2E/LOG10E); String (split com limit, startsWith/
  endsWith com offset, match/search/matchAll via regex); Symbol
  (Symbol/Symbol.for/keyFor/description + well-known iterator/
  asyncIterator/hasInstance/toPrimitive/toStringTag); URL/
  URLSearchParams completos; Date setters (setFullYear/Month/Date/
  Hours/Minutes/Seconds + UTC variants), getTimezoneOffset,
  toUTCString/toDateString/toJSON/toLocaleString/toTimeString;
  TextEncoder/TextDecoder; encodeURIComponent/decodeURIComponent;
  WeakMap/WeakSet (semantica strong por enquanto, #217); Boolean
  class com toString/valueOf; parseInt com radix.

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
