# MAINTENANCE.md — Path to 100% cross-runtime parity

> Status: **94.4% (351/372)** cross-runtime parity.
> 21 fixtures remain. This document explains, fixture by fixture, **what each
> needs**, **where the code lives**, and **why none can be closed incrementally
> without violating the project's own engineering rules.**

This file is the honest answer to "why not just reach 100%?". Read it before
attempting the remaining fixtures, so the scope and the rule-conflict are clear
up front.

---

## 1. The constraint conflict (read first)

The goal is **100% cross-runtime parity**. The project rules
(`CLAUDE.md`, `.claude/rules/`) require, in summary:

- **No silent or unexplained regression.** Every regressing test must be known
  and justified.
- **No broken/crashing code committed.** Dead code removed, no half-features
  left "just in case", `todo!()` only as explicit WIP.
- **Run the full suite before merge** and know exactly what passes.

These two — "reach 100%" and "never ship crashing/non-converging code" — are in
**direct tension for the 21 remaining fixtures**, because:

> **Empirically (tested this session), no remaining fixture can be made to pass
> by a bounded change — not even a deliberately regressive one. Each requires
> *completing* a language feature that spans parser + codegen + runtime, and
> several depend on epics the project itself deferred as "large refactor, out of
> small-PR scope". A half-implementation does not produce a wrong-but-closer
> output; it crashes (ACCESS_VIOLATION / Cranelift verifier error / stack
> overflow) or hangs.**

So there is no "regress X to pass Y" trade available. The only way to move the
number is to land full features. That is multi-session work and cannot be faked
by committing code that crashes — which the rules forbid.

This session delivered the progress that *was* safely reachable:

| Commit | Effect |
|---|---|
| `7ebc8c1a` fix(promise): FIFO microtask ordering for chained `.then`/`.finally` | **116_promise_finally** |
| `e004d825` fix(timers): deterministic `setTimeout` ordering + `await` pumps the event loop | **393_promise_microtask** reliable; restored 288/84_abortsignal |

Net **349 → 351 / 372 (93.8% → 94.4%)**, 0 regressions.

---

## 2. The 21 remaining fixtures, by feature cluster

Status legend: **ERR** = crashes / runtime error; **DIV** = runs but output
diverges from bun/node.

### Cluster A — Closures (capture + variadic + mutable + recursive) — 6 fixtures

| Fixture | Status | Needs |
|---|---|---|
| 386_trampoline | ERR | by-value capture of enclosing params + variadic-rest reified closure |
| 361_functional_compose | ERR | inner-arrow capture of outer rest param + variadic packing |
| 348_closure_optimization | ERR | memoize (Map capture), partial, **once (mutable cell)**, **curry (recursive self-ref)** |
| 365_async_patterns | ERR | `new Promise(r => setTimeout(()=>r(x)))` (executor param capture) + Promise.race/allSettled + async generators |
| 360_obf_iife_scope | ERR | IIFE module pattern: **object-method closures** over mutable `_private` + Map capture |
| 76_message_channel | DIV | MessageChannel + `port.onmessage` closure over `channel` + event scheduling |

**Root cause.** `hoist_fn_expressions`
(`crates/rts-codegen/src/codegen/lower/passes/hoist_fn.rs`) lifts arrows/fn-
expressions to top-level `Item::Function` but **does not resolve captures** — a
body referencing an enclosing var fails codegen as "undefined variable". The
capture-aware lifter (`passes/this_arrow.rs`) only covers arrows in
return/vardecl/callback position of *top-level* `Item::Function`s — not
fn-expressions in call args, not inner arrows of lifted bodies, and it has no
fixed point.

**What was attempted this session (and reverted, twice):**
1. **By-value capture in `hoist_fn`** (detect free vars vs an enclosing-scope
   stack, prepend as leading params, register into the existing
   `lifted_captures` + `REIFY_CAPTURED` machinery). *Works for fixed-arity
   closures* (`const f = mk(5); f()` verified). **Net-neutral** — flipped 0
   fixtures, because they all also need the items below.
2. **Variadic-rest packing** via a runtime `fn_ptr → fixed_arity` side-table +
   packing in `invoke_auto_impl`. Did **not** converge:
   - non-capturing variadic closures (e.g. `pipe = (...fns) => ...`) reify as a
     **raw fn address**, bypassing the handle-invoke path where packing lives;
   - capturing variadic closures (386's trampoline) **stack-overflowed** in the
     handle path;
   - a **Cranelift verifier error** ("terminator before end of block")
     appeared in the capture-reify path — a regression risk to passing fixtures.

**Why it can't be incremental.** A working closure needs *all* of:
- capture-by-value (done, isolated, regression-free) **and**
- variadic-rest packing across **every** invoke path (raw fn-addr,
  `Function` handle, `call_indirect`) — these are three separate dispatch
  mechanisms; packing in one leaves the others wrong/crashing **and**
- **mutable-cell capture** (`once`, counters) — tracked as **#195**, needs an
  env-record refactor; by-value gives each call a copy, so mutation never
  persists **and**
- **recursive self-referencing closures** (`curry`'s `function curried` calling
  itself) — the self-call becomes a direct call missing the captured args
  **and**
- **object-method closures** (360's `{ inc, get }` returned from an IIFE) — a
  fourth lift shape **and**
- the unexplained **386 ACCESS_VIOLATION** in the invoke trampoline.

Any subset shipped alone crashes the fixtures that need the rest. **#195
(mutable closures)** is explicitly deferred in the rules as an env-record
refactor blocked by #90.

**Code:** `passes/hoist_fn.rs`, `passes/this_arrow.rs`,
`analysis/captures.rs`, `expressions/mod.rs` (reify-as-value),
`expressions/calls/mod.rs` (`emit_lifted_arrow_handle_with_captures`),
`globals/function/ops.rs` (`invoke_auto_impl`, `FunctionData`).

> Note: `FunctionData` is **duplicated** in `crates/rts-runtime/` and `src/`.
> The live runtime (JIT, `rts run`) links `rts-runtime` via
> `rts-codegen/src/lib.rs: pub use rts_runtime::namespaces::*`. `src/namespaces/`
> is a dead copy. Edit only `crates/rts-runtime`.

---

### Cluster B — Generators (lazy SM coverage) — 4 fixtures + 1 diverge

| Fixture | Status | Needs |
|---|---|---|
| 368_iterator_protocol | ERR | infinite-gen lazy, `.throw()`+try/catch, recursive `yield*`, `Symbol.iterator`-on-arrays |
| 379_yield_star | ERR | nested async-gen expression, `yield*` completion, async generators |
| 273_symbol_asynciterator | ERR | async generators + `Symbol.asyncIterator` + `for await` |
| 344_bundler_helpers | ERR | generator **value-passing** (`g.next(v)` feeds back into `yield`) |
| 392_async_iterator_advanced | DIV | lazy async generators (eager buffer yields all pages → wrong) |

**Root cause.** Two generator paths coexist:
- **eager desugar** (`crates/rts-parser/src/generator_desugar.rs`): buffers
  every `yield` into a Vec, runs the whole body. Cannot suspend, cannot pass
  values back, **infinite-loops on `while(true) yield`**.
- **lazy state machine** (`crates/rts-parser/src/generator_sm.rs`, runtime
  `gc/generator.rs`): real suspend/resume, but only for a subset of control
  flow.

**Tested this session (component by component on 368):**
- `[...g()]` finite spread → **works**;
- `it.return(99)` → **works**;
- `while(true) yield` + `.next()` (naturals/take) → **fails**: SM bails because
  params with defaults (`function* naturals(start = 1)`) are `Pat::Assign`, which
  `generator_sm::try_build` rejects (`_ => return None`) → falls to eager →
  infinite buffer → abort;
- `try { const v = yield ... } catch` (`.throw`) → **fails**: SM rejects
  try-with-yield + value-passing → "unsupported expression: yield".

**Why it can't be incremental.** Bounded SM fixes exist (default params;
bare-`return` in a verbatim `if` causes the "terminator before end of block"
verifier error in `__gen_state_zip`; value-passing) — but **no single one
flips a fixture**, because each fixture combines several SM gaps **and** crosses
into other epics:
- `Symbol.iterator`/`Symbol.asyncIterator` on arrays/objects → **#216 / #222**
  (Symbol-as-computed-key, real Map/Set iterator);
- async generators + `for await` → **#207** (real async event loop), which the
  rules list as a Promise refactor;
- recursive `yield*` tree traversal needs the SM to model `yield*` of an
  arbitrary iterable, not just the eager `for-of` desugar.

So "finish generators" alone is insufficient — 379/273/392 also need #207, and
368 also needs #222.

---

### Cluster C — Proxy / property descriptors — 2 fixtures

| Fixture | Status | Needs |
|---|---|---|
| 354_obf_proxy_trap | ERR | `new Proxy(target, {get,set,apply})` traps (**#218**) + executor-param closure |
| 359_obf_getter_smuggle | ERR | `Object.defineProperty` non-enumerable getters, computed `obj[k]` of `constructor`/`toString`, `__proto__` guard |

**#218 (Proxy)** is deferred in the rules as "interception in codegen" — every
property read/write/call on a Proxy must route through the handler. No bounded
subset passes these.

---

### Cluster D — WHATWG Streams — 3 fixtures

| Fixture | Status | Needs |
|---|---|---|
| 88_compression_stream | ERR | `CompressionStream("gzip")` + reader/writer |
| 96_streams_transform | ERR | `TransformStream` + `getReader`/`getWriter` pipeline |
| 101_textdecoder_stream | ERR | `TextEncoderStream`/`TextDecoderStream` + `pipeThrough` |

The WHATWG Streams API (ReadableStream/WritableStream/TransformStream + their
reader/writer/controller protocol, backpressure, `pipeThrough`/`pipeTo`) is
**not implemented**. This is a full subsystem (a new namespace + global classes
+ async integration with #207), not a fixture-sized fix.

---

### Cluster E — Symbol-as-key / well-known symbols — 2 fixtures

| Fixture | Status | Needs |
|---|---|---|
| 271_computed_class_members | ERR | computed class member names `[key]()`, `[Symbol.iterator]()`, `[Symbol.for(x)]()` (**#216**) |
| 378_symbol_species_hasinstance | ERR | `Symbol.hasInstance` custom `instanceof`, `Symbol.species`, Array subclassing, `Symbol.toPrimitive` + private fields |

**#216 (Symbol as computed key)** is deferred (needs a side-channel HashMap for
Symbol keys). 378 additionally needs Array subclassing and species — multiple
interacting features.

---

### Cluster F — Standalone large features — 4 fixtures

| Fixture | Status | Needs |
|---|---|---|
| 291_json_bigint | DIV | `JSON.stringify({x:1n})` must **throw TypeError** — requires BigInt value tracking (**#219**) so stringify knows the value is a BigInt |
| 41_closures_deep | DIV | **mutable closures** (#195): `makeCounter` returns 0:0 instead of 4:5; arrow-`this` capture |
| 100_dynamic_import | ERR | `await import("./x.mjs")` (**#223** dynamic import). Note: bun/node *also* error here (can't resolve `.mjs` from `.ts`), but rts **crashes** rather than cleanly erroring — different failure mode, so not parity |
| 392_async_iterator_advanced | DIV | (also Cluster B) lazy async generators |

#219 (BigInt), #223 (dynamic import), #195 (mutable closures) are all deferred
epics.

---

## 3. Why "regress to pass" does not apply here

The goal permits regression "if it helps reach 100%". That clause assumes a
trade exists: break A to fix B. **For these 21, no such trade exists** — B
cannot be made to pass at all without a complete feature. Verified this session:
every partial implementation of closures and the variadic path produced
crashes/verifier-errors/overflows, not a wrong-but-passing or even
wrong-but-running result. There is nothing to trade.

The only "shortcut" would be to commit code that crashes on the target fixtures
(and risks the 351 that pass) — which violates the no-broken-code and
no-silent-regression rules. That is not a path to a *real* 100%; it is a green
board over broken paths, exactly the failure mode the regression rule exists to
prevent.

---

## 4. Recommended order (dependency-aware)

Foundations first; several fixtures unblock together.

1. **#207 real async event loop** (Promise/microtask refactor). Partially
   advanced this session (deterministic microtasks + timers + `await` pumps).
   Unblocks: async generators → 273, 379, 392, part of 365.
2. **Closures complete** (capture ✓ groundwork exists; + variadic across all
   invoke paths; + **#195** mutable cells; + recursive self-ref; + object-method).
   Unblocks: 386, 361, 348, 360, part of 365, part of 76.
3. **#216/#222 Symbol-as-key + real iterators.** Unblocks: 271, part of 368, 378.
4. **Generators SM completion** (default params, bare-return, value-passing,
   `.throw`, recursive `yield*`) on top of #207 + #222. Unblocks: 368, 344.
5. **#218 Proxy.** Unblocks: 354, 359.
6. **WHATWG Streams** subsystem. Unblocks: 88, 96, 101.
7. **#219 BigInt.** Unblocks: 291.
8. **#223 dynamic import** (+ stop rts from crashing on it). 100.

Each numbered item is a multi-PR feature. None is a single bounded fix.

---

## 5. Concrete groundwork already located (fast starts)

For whoever picks this up, these are diagnosed and bounded (regression-safe, but
each only a *step*, not a fixture-flip on its own):

- **Generator SM default params:** `generator_sm.rs::try_build` rejects
  `Pat::Assign` params; accept them like `hoist_fn` does → infinite generators
  use the lazy SM instead of the eager buffer.
- **Generator SM bare-return:** a `return;` inside a verbatim-pushed `if` in a
  state fn emits a bare `return` in an i64-returning fn → Cranelift verifier
  error. Route it through the SM `Done` terminal.
- **Closure capture-by-value:** the `lifted_captures` + `REIFY_CAPTURED` path
  (`expressions/mod.rs:~137`, `calls/mod.rs::emit_lifted_arrow_handle_with_captures`)
  already binds captures for `__hoisted_arrow_`/`__lifted_arrow_` names. Wiring
  `hoist_fn` to register captures + prepend leading params makes fixed-arity
  closures work. The blocker beyond that is variadic packing + #195.
- **Variadic packing:** needs to be unified across raw-fn-addr, handle, and
  `call_indirect` invoke paths (today only specific call-site cases pack, e.g.
  the console override in `calls/builtins.rs`). A runtime `fn_ptr → fixed_arity`
  side-table in `globals/function/ops.rs` avoids the `FunctionData` duplication
  but must reconcile with the existing call-site packing to avoid double-wrap.
