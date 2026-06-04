# Active features — capabilities, parallelism, async/Promise/Function

## Pipeline HIR → MIR → Cranelift (hybrid routing, default ON)

Phase 3 of `RTS_REFACTOR.md` delivered: the `rts-mir` crate is active by default
(commits f7b924b/23dd4b7). Current pipeline:

```
TS → SWC → AST → HIR (rts-hir) → MIR (rts-mir) → inline (fixed-point) → optimize (fold→fma→cse→dce) → mir_codegen → Cranelift
                                              ↘ authoritative AST (fallback)
```

Phase 4 in progress (5/8 delivered): atomics in MIR (4.1), inline + integration +
fixed-point (4.2/4.3/4.7), intra-block CSE (4.5), FMA fusion `a*b+c → Fma` (4.8),
e2e smoke + arr[i]=v (4.4/4.6). Remaining: escape analysis, SIMD, real narrow
storage.

Each user fn tries the HIR→MIR→Cranelift path; on an unmodeled construct (member
on `this`/objects, classes, async/await, address-taken fns, string in user-fn
params/ret) it falls back automatically to AST codegen. The bail is silent and
does not break semantics.

`RTS_USE_MIR` variable:

| Value | Behavior |
|---|---|
| unset / `1` / `on` / `all` | MIR ON (default) |
| `0` / `off` / `none` | AST only |
| `fn1,fn2,...` | MIR only for the listed fns |

**MIR capabilities:** literals (int/float/bool/string/null/undefined),
integer/float arithmetic, bitwise, shifts, comparisons, casts; full control flow
(if/else, while, do-while, classic for, break/continue with loop_stack,
ternary→Select, switch via br_table, throw→Trap, try/finally phase 1); SSA
mutation via block params (`let i = i + 1` in loops); cross-fn calls (CallUser) +
mutual recursion; self-recursion bail (TCO only on AST); extern calls to RTS
namespaces via `CallExtern` + resolve via SPECS; intrinsic inlining (math.sqrt,
abs/min/max f64/i64); string literals (`StrLit` data segment + StrPtr ptr+len);
namespace constants (math.PI, math.E); simple arrays via `collections.vec_*`;
automatic GC stack maps via `declare_value_needs_stack_map`.

**Current metrics:** 438 real user fns from the TS suite run through MIR.
`cargo test --release --lib` 12/12; `rts-hir` 27/27; `rts-mir` **59/59** (+8 vs
Phase 3 final, covering inline/CSE/FMA); `rts-codegen --lib mir_codegen`
**61/61** (+8 vs Phase 3); `rts.exe test` 622/632 (same 10 pre-existing AST
failures).

## Active language capabilities (codegen)

- Object/array literals: `{k: v}` and `[1,2,3]` via `collections.map_*`/`vec_*`.
- Classes: constructor, method, this, extends, super(args), super.method(args),
  static methods, getters/setters. Instance stores `__rts_class` for real virtual
  dispatch (subclass override routed via string comparison over the runtime tag).
- Rust-style operator overload: `a + b` becomes `a.add(b)` at compile time when
  the class defines the method (`add`/`sub`/`mul`/.../`eq`/`lt`/`bit_*`).
- `for...of` over arrays; bind inherits the class when the array has a `: C[]`
  annotation.
- try/catch/finally phase 1: thread-local error slot, no real unwind (#128 tracks
  phase 2).
- String equality: `s1 == s2` compares content via `gc.string_eq`.
- async/await with a Promise-centric pipeline (see section below).
- Function class — `.call/.apply/.bind/.toString` + `.name/.length` + `new
  Function("body")` via runtime eval.
- Destructuring (#210): array/object, defaults, rest, nested, in fn/arrow params,
  in for-of, in catch, alias `{a: b}`.
- Expanded JS builtins (epic #226 in progress): Array (indexOf/lastIndexOf/
  includes with fromIndex, reverse, shift/unshift, slice, concat, fill,
  flat/flatMap, splice, findLast/findLastIndex, reduceRight, copyWithin, sort
  with strings, values/keys/entries, toSorted/toReversed/toSpliced/with,
  Array.from(length) and Array.from(arr)); Object (entries, assign, freeze,
  fromEntries, seal, isFrozen, isSealed, getPrototypeOf, defineProperty); Math
  (sign, hypot, expm1, log1p, fround, sinh/cosh/tanh/asinh/acosh/atanh, imul,
  clz32 + SQRT2/SQRT1_2/LN2/LN10/LOG2E/LOG10E); String (split with limit,
  startsWith/endsWith with offset, match/search/matchAll via regex); Symbol
  (Symbol/Symbol.for/keyFor/description + well-known iterator/asyncIterator/
  hasInstance/toPrimitive/toStringTag); full URL/URLSearchParams; Date setters
  (setFullYear/Month/Date/Hours/Minutes/Seconds + UTC variants),
  getTimezoneOffset, toUTCString/toDateString/toJSON/toLocaleString/
  toTimeString; TextEncoder/TextDecoder; encodeURIComponent/decodeURIComponent;
  WeakMap/WeakSet (strong semantics for now, #217); Boolean class with
  toString/valueOf; parseInt with radix.

## Silent parallelism (Level-1)

Codegen has 3 passes that rewrite common TS patterns to `parallel.*` calls
automatically. The user does not need to mention threads/workers:

- **`array_methods_pass`** — detects `arr.map(fn)`, `arr.forEach(fn)`,
  `arr.reduce(fn, init)` when `fn` is the Ident of a user fn → rewrites to
  `parallel.map/for_each/reduce`. Runs first.
- **`reduce_pass`** — detects the classic accumulator pattern (`let s = 0; for (x
  of arr) s = s + EXPR;` or `s += EXPR`) and rewrites to `parallel.reduce`. Only
  accepts associative ops (+, *).
- **`purity_pass`** — detects `for...of` whose body only calls `pure: true`
  namespace members and has no assignments → rewrites to `parallel.for_each`.

The 3 passes cover top-level + the body of each user fn. Shared counters with no
name collision. 96 fns marked `pure: true` today (math, string, num, fmt, path,
hash, mem) — the recognition base.

`parallel.*` accepts literal arrays, arrays in a variable, and arrays returned
from a fn (all become Vec<i64> via array-literal codegen). A bridge for Buffer
and typed arrays is a follow-up.

Detailed spec: `docs/specs/silent-parallelism.md`.

## async / Promise / Function (Promise-centric, #437)

Unified subsystem implemented in PRs #428-#437. Design proposed by @drysius:
Promise concentrates spawn+state; Function is the payload (fn_ptr + bound_args).

### `async function` pipeline

```
async function f(a, b) { body }

=> (after expand_async_functions)

function __async_inner_f(a, b): i64 { body }
function f(a, b): i64 {
    const __args = [a, b];
    return promise.create(__async_inner_f, __args);
}
```

`promise.create(fn_handle, args_vec)` in Rust:
- Allocates a pending PromiseAsync
- Resolves the fn (Function handle or direct ptr via `resolve_callback_ptr`)
- `rt.spawn_blocking(move || invoke + settle)`
- Automatic settle: `resolve(retval)` or `reject(err)` based on a thread-local
  error slot

`await x` → `promise.wait(x)` (blocks on a oneshot, propagates rejection via the
error slot).

### Function class

- `Entry::Function { fn_ptr, arity, name, bound_this, bound_args, is_arrow,
  source, keep_alive }`
- `__RTS_FN_GL_FUNCTION_REIFY/CALL/APPLY/BIND/NAME/LENGTH/TO_STRING/NEW`
- The `invoke_n` trampoline `transmute`s to `extern "C" fn(i64...) -> i64` by
  arity up to 8
- `new Function("a", "body")` via `eval_compile::compile_function` → parser+JIT
  pipeline → JITModule kept alive via `Arc<Mutex<dyn Any>>` in `keep_alive`

### Promise+Function integration

- `promise.then(p, fn)` accepts a direct ptr OR a Function handle (from `bind()`
  or `new Function`)
- `resolve_callback_ptr` detects the type via `Entry::Function` lookup
- `userFn.call(...)` in an async fn returns a Promise (because `f` was rewritten
  to return `promise.create(...)`)

### Conscious limitations

- `this` binding in non-arrow fn: thisArg ignored in `.call(thisArg)` (RTS user
  fns have no reserved slot)
- `fn.toString()` on a static user fn: `"function name() { [native code] }"`
  (source not preserved)
- `fn.prototype`, `arguments`: do not exist
- `new Function("async ...")`: native-mode eval does not allow async at top-level
- `invoke_n` arity > 8: panic (rare in practice)

Detailed spec: `docs/specs/async-promise-function.md`.

## Regression discipline

Review `MANDATORY RULE: REGRESS WHEN NECESSARY (EXPLICITLY)` in `00-meta.md`.
Regression is allowed when necessary, as long as it is always explicit and
justified — never silent. That is what keeps the suite reliable across hundreds
of PRs.
