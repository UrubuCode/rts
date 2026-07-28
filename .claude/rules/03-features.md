# Engine model + target semantics — value model, async/Promise/Function

> Canonical design: `docs/specs/rts-codegen-new-design.md`. This file describes
> the engine's value model and the JS/TS **semantics it must cover**. The
> runtime trampolines (`PolyValue`, `__rtsadp_*`) live in
> `crates/rts-runtime/src/adapters/` (folded in from the former standalone
> `rts-adapters` crate); the lowering-time slices (Repr lattice, shapes,
> codegen-state reset) live in `crates/rts-codegen-new/` (the design doc's
> `src/*.rs` path map is stale — trust the tree on disk).

## The value model (single engine, no MIR, no dual codegen)

Single lowering path `HIR → Cranelift IR`; the Cranelift egraph
(`use_egraphs=true`) is the **sole** optimizer. The four pillars: PolyValue is
`rts-runtime/src/adapters/`; Repr/shapes/dispatch are the lowering-time slices
in `rts-codegen-new` (`adapters::dispatch` stays in `rts-runtime` — a runtime
trampoline reads it too, and this is now an ordinary same-crate module, not a
cross-crate dependency):

- **PolyValue (`value/`, Pilar 1)** — one 64-bit NaN-boxed word. A bit-pattern
  with `(bits & BOX_BASE) == BOX_BASE` (`BOX_BASE = 0xFFF8_0000_0000_0000`, the
  negative-qNaN quadrant) is **boxed**; anything else is a genuine inline `f64`.
  Boxed = 3-bit tag (bits 50..48) + 48-bit payload: `INT32(1)`, `SINGLETON(2)`
  (undefined/null/false/true/hole/empty), `STR(3)`, `OBJECT(4)`, `FUNCTION(5)`
  (0/6/7 reserved for symbol/bigint/future). The 48-bit payload of STR/OBJECT/
  FUNCTION is a **HandleTable slot index** (slot+shard), never a raw pointer —
  which is what makes NaN-boxing GC-safe. `from_f64` canonicalizes every NaN to
  the *positive* qNaN so real doubles never collide with the boxed space.
  `typeof` is a single tag inspection. This **deletes** the 4 compile-time
  side-tables, `Entry::FloatPrim`, and the `__RTS_FN_RT_FLOAT_BOX/UNBOX/EQ/ARITH`
  helpers.
- **Repr lattice (`repr.rs`, Pilar 2)** — every IR value has exactly ONE `Repr`:
  `Int32` / `Float64` / `Bool` / `Ref(RefKind)` / `Tagged`. A value is kept
  **unboxed** (in a register — the winning numeric path) only where the front-end
  PROVES monomorphism (validated TS annotations at untrusted boundaries,
  literals, local flow). `join(a, b) = if a == b { a } else { Tagged }` — a
  total, decidable rule. box/unbox are **explicit IR nodes** at proven
  boundaries, never "tracked elsewhere" in a `HashSet`. Hard points (loop-header
  phis, catch bindings, destructuring, closure captures, generator state) all
  default conservatively to `Tagged` — correct, never silently wrong.
- **Shapes + data ICs (`shape.rs` in rts-codegen-new, Pilar 4)** — objects are
  `{ shape_id, slots: [PolyValue; N] }`. Property access = compare `shape_id` +
  fixed-offset load (not hash lookup); construction walks a transition tree so
  same-key-sequence objects share a shape; method dispatch is **shape-keyed, not
  O(N) `gc.string_eq`**. Inline caches are **data cells** (`PropIcCell`:
  `{shape, slot, state}`) the emitted code loads and compares — AOT-safe, no
  self-modifying code; state machine `uninit → mono → poly → mega`. The flat
  layout is the **default**; `HashMap` dictionary mode only for pathological
  objects (mass computed keys, frequent `delete`). This deletes the default
  `HashMap<String,i64>` property-bag and the string-compare vtable.
- **Soundness rule (Pilar 3)** — *unbox on a representation PROVED at the point;
  insert runtime tag-checks at untrusted boundaries* (exported-fn params, `any`,
  `JSON.parse` results, Registry-resolved returns). Inside a proven region: no
  checks (the fast path). Polymorphic `+`: both-proven-number → native
  `iadd`/`fadd`; otherwise ONE `ADD_GENERIC` running the real JS `+` algorithm,
  with an inline tag-check fast path for the secretly-monomorphic case — **never**
  AST-shape guessing (`is_map_get_call` and friends die here).

## Target semantics the engine must cover

The JS/TS surface the engine must support, via PolyValue/shapes/ICs. The
per-category list:

- Object/array literals.
- Classes: constructor, method, this, extends, super(args), super.method(args),
  static methods, getters/setters. Virtual dispatch must be shape-keyed (the old
  engine used a `__rts_class` string tag + O(N) string compare — replaced).
- Rust-style operator overload: `a + b` → `a.add(b)` at compile time when the
  class defines the method (`add`/`sub`/`mul`/…/`eq`/`lt`/`bit_*`).
- `for...of`; try/catch/finally phase 1 (thread-local error slot, no real unwind,
  #128 tracks phase 2); string equality.
- async/await Promise-centric (see section below).
- Function class — `.call/.apply/.bind/.toString` + `.name/.length` + `new
  Function("body")` via runtime eval.
- Destructuring (#210): array/object, defaults, rest, nested, in fn/arrow params,
  in for-of, in catch, alias `{a: b}`.
- Expanded JS builtins (epic #226): Array (indexOf/lastIndexOf/includes with
  fromIndex, reverse, shift/unshift, slice, concat, fill, flat/flatMap, splice,
  findLast/findLastIndex, reduceRight, copyWithin, sort, values/keys/entries,
  toSorted/toReversed/toSpliced/with, Array.from); Object (entries, assign,
  freeze, fromEntries, seal, isFrozen, isSealed, getPrototypeOf, defineProperty);
  Math (sign, hypot, expm1, log1p, fround, hyperbolic, imul, clz32 + consts);
  String (split with limit, startsWith/endsWith offset, match/search/matchAll);
  Symbol (Symbol/for/keyFor/description + well-known); full URL/URLSearchParams;
  Date setters + getTimezoneOffset + toUTCString/toDateString/toJSON/…;
  TextEncoder/TextDecoder; encode/decodeURIComponent; WeakMap/WeakSet (strong
  semantics for now, #217); Boolean class; parseInt with radix. Every
  non-primordial method resolves via the Registry/`MethodSpec` (no builtins in
  the engine).

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
