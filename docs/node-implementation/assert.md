# node:assert

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:assert` (+ `node:assert/strict` entry point) |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P0 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import assert from "node:assert"` · `import assert from "node:assert/strict"` · `import { strict as assert } from "node:assert"` · `const assert = require("node:assert")` · `const assert = require("node:assert").strict` · `const assert = require("node:assert/strict")` |
| Globals exposed | none |

## 1. Purpose

`node:assert` provides a set of synchronous assertion functions used to verify
invariants at runtime — the backbone of Node's own test suite and of most
userland test frameworks written against Node's built-ins. It has two modes
(legacy loose and strict) that differ only in the equality algorithm used by
the non-`*Strict*`-named functions, plus a small set of always-strict
functions (`strictEqual`, `deepStrictEqual`, `throws`, `rejects`, …) whose
behavior is identical in both modes. Because it operates purely on already
materialized JS values (no I/O, no OS calls), it is one of the simplest Node
modules to port and is entirely expressible as engine-level value operations.

## 2. Exported API surface (COMPLETE)

### Classes

#### `assert.AssertionError`

Extends: `Error`.

- **Constructor:** `new AssertionError(options: AssertionErrorOptions)`
  - Throws nothing itself; always constructs successfully given a plausible
    `options` object (malformed shapes fall back to sane defaults, mirroring
    Node's tolerant constructor).
- **Instance properties:**
  - `message: string` — final formatted message (custom `options.message` or
    an auto-generated diff-based message).
  - `name: string` — always `"AssertionError"`.
  - `actual: any` — the actual value passed in (or `undefined`).
  - `expected: any` — the expected value passed in (or `undefined`).
  - `operator: string` — the assertion operator name, e.g. `"strictEqual"`,
    `"deepStrictEqual"`, `"=="`, `"fail"`.
  - `generatedMessage: boolean` — `true` when `message` was auto-generated
    (i.e. the caller did not pass a custom `message`).
  - `code: string` — always `"ERR_ASSERTION"`.
  - Inherited from `Error`: `stack`, `cause` (when supplied via
    `options.message` being an `Error`-shaped cause chain is NOT auto-set;
    `cause` only appears if the caller's custom logic sets it — assert itself
    does not populate `cause`).
- **Events:** none (not an `EventEmitter`).

#### `assert.Assert`

*(Added Node v24.6.0 / v22.19.0.)* A class that produces an independent,
configurably-stricter assertion object; every module-level assertion function
below is also an instance method with identical signature and semantics,
except that its behavior is parameterized by the instance's `options`.

- **Constructor:** `new Assert(options?: AssertOptions)`
  - `options.diff?: 'simple' | 'full'` — default `'simple'`.
  - `options.strict?: boolean` — default `true`. When `true`, the instance's
    non-strict-named methods (`equal`, `notEqual`, `deepEqual`,
    `notDeepEqual`) behave like their `*Strict*` counterparts (mirrors module
    `assert.strict`). When `false`, they use legacy loose comparison.
  - `options.skipPrototype?: boolean` — default `false` *(added v24.9.0)*.
    When `true`, `[[Prototype]]`/constructor identity is **not** compared
    during deep equality, so instances of structurally-identical but
    differently-named classes can compare equal.
- **Instance methods:** `ok`, `equal`, `notEqual`, `strictEqual`,
  `notStrictEqual`, `deepEqual`, `notDeepEqual`, `deepStrictEqual`,
  `notDeepStrictEqual`, `partialDeepStrictEqual`, `match`, `doesNotMatch`,
  `throws`, `doesNotThrow`, `rejects`, `doesNotReject`, `fail`, `ifError` — all
  with the same per-function signatures as section "Top-level functions"
  below, but bound to `this` instance's `diff`/`strict`/`skipPrototype`
  configuration.
- **Caveat (must be preserved in the .ts shim):** destructuring an instance
  method off an `Assert` instance (`const { deepStrictEqual } = new
  Assert(...)`) loses access to the instance's configuration unless the
  method captures its owning instance via closure at construction time (i.e.
  each instance method must be created as a bound closure over `this`
  configuration, not a shared prototype method that reads `this` at call
  time) — otherwise calling the destructured reference with no receiver
  silently reverts to default options instead of erroring or preserving
  config.
- **Events:** none.

### Top-level functions

All are plain synchronous functions unless noted (`rejects`/`doesNotReject`
return `Promise<void>`). `message` is accepted by every function; when it is
an `Error` instance it is thrown as-is instead of being wrapped in an
`AssertionError`. All non-`Promise`-returning functions throw
`AssertionError` (`code: "ERR_ASSERTION"`) on failure and return `void` on
success.

| # | Signature | Variant |
|---|---|---|
| 1 | `assert(value: any, message?: string \| Error): void` | sync |
| 2 | `assert.ok(value: any, message?: string \| Error): void` | sync |
| 3 | `assert.equal(actual: any, expected: any, message?: string \| Error): void` | sync |
| 4 | `assert.notEqual(actual: any, expected: any, message?: string \| Error): void` | sync |
| 5 | `assert.strictEqual(actual: any, expected: any, message?: string \| Error): void` | sync |
| 6 | `assert.notStrictEqual(actual: any, expected: any, message?: string \| Error): void` | sync |
| 7 | `assert.deepEqual(actual: any, expected: any, message?: string \| Error): void` | sync |
| 8 | `assert.notDeepEqual(actual: any, expected: any, message?: string \| Error): void` | sync |
| 9 | `assert.deepStrictEqual(actual: any, expected: any, message?: string \| Error): void` | sync |
| 10 | `assert.notDeepStrictEqual(actual: any, expected: any, message?: string \| Error): void` | sync |
| 11 | `assert.partialDeepStrictEqual(actual: any, expected: any, message?: string \| Error): void` | sync |
| 12 | `assert.match(str: string, regexp: RegExp, message?: string \| Error): void` | sync |
| 13 | `assert.doesNotMatch(str: string, regexp: RegExp, message?: string \| Error): void` | sync |
| 14 | `assert.throws(fn: () => void, error?: RegExp \| Function \| object \| Error, message?: string): void` | sync |
| 15 | `assert.doesNotThrow(fn: () => void, error?: RegExp \| Function, message?: string): void` | sync |
| 16 | `assert.rejects(asyncFn: (() => Promise<any>) \| Promise<any>, error?: RegExp \| Function \| object \| Error, message?: string): Promise<void>` | promise |
| 17 | `assert.doesNotReject(asyncFn: (() => Promise<any>) \| Promise<any>, error?: RegExp \| Function, message?: string): Promise<void>` | promise |
| 18 | `assert.fail(message?: string \| Error): never` | sync (always throws) |
| 19 | `assert.ifError(value: any): void` | sync |

Per-function detail:

- **`assert(value, message?)` / `assert.ok(value, message?)`** — Params:
  `value: any` (required, not optional), `message?: string | Error`. Returns
  `void`. Throws `AssertionError` when `!value` is truthy (i.e. `value` is
  falsy). Semantically `assert.ok(v, m)` ≡ `assert.equal(!!v, true, m)`
  (implemented directly as a truthiness check, not by calling `equal`).
  `assert()` is a direct alias/re-export of `assert.ok`.

- **`assert.equal(actual, expected, message?)`** — Legacy mode: `actual ==
  expected` (coercive), with `NaN == NaN` treated as equal (deviating from
  real `==`). Strict mode (via `assert.strict`/`Assert{strict:true}`):
  identical to `strictEqual`. Params: `actual: any`, `expected: any`,
  `message?: string | Error`. Returns `void`. Throws `AssertionError`,
  `operator: "=="` (legacy) or `"strictEqual"` (strict).

- **`assert.notEqual(actual, expected, message?)`** — Inverse of `equal`
  (`actual != expected` legacy; `notStrictEqual` in strict mode). Same params.

- **`assert.strictEqual(actual, expected, message?)`** — `Object.is(actual,
  expected)`. `0` and `-0` are distinct; `NaN` equals `NaN`. Params/return as
  above. `operator: "strictEqual"`.

- **`assert.notStrictEqual(actual, expected, message?)`** — `!Object.is(actual,
  expected)`. `operator: "notStrictEqual"`.

- **`assert.deepEqual(actual, expected, message?)`** — Legacy recursive
  structural equality (stability: Legacy, kept for back-compat; strict mode
  makes this behave like `deepStrictEqual`). See §3/§4 for the full
  comparison algorithm. `operator: "deepEqual"`.

- **`assert.notDeepEqual(actual, expected, message?)`** — Inverse of
  `deepEqual`. `operator: "notDeepEqual"`.

- **`assert.deepStrictEqual(actual, expected, message?)`** — Strict recursive
  structural equality (always the same algorithm regardless of legacy/strict
  mode). `operator: "deepStrictEqual"`.

- **`assert.notDeepStrictEqual(actual, expected, message?)`** — Inverse.
  `operator: "notDeepStrictEqual"`.

- **`assert.partialDeepStrictEqual(actual, expected, message?)`** — Like
  `deepStrictEqual`, but only properties present on `expected` are checked
  (subset match); `actual` may have extra own properties not present on
  `expected`. Stable since v24.0.0/v22.17.0 (added v23.4.0/v22.13.0).
  `operator: "partialDeepStrictEqual"`.

- **`assert.match(str, regexp, message?)`** — Throws unless `typeof str ===
  "string"` and `regexp.test(str)` is `true`. `operator: "match"`.

- **`assert.doesNotMatch(str, regexp, message?)`** — Throws unless `typeof
  str === "string"` and `regexp.test(str)` is `false` (i.e. throws if it IS a
  string and it DOES match, or if it is not a string at all).
  `operator: "doesNotMatch"`.

- **`assert.throws(fn, error?, message?)`** — Calls `fn()`; throws
  `AssertionError` if `fn` does not throw, or if it throws but the thrown
  value does not satisfy `error` (when provided). `error` may be:
  - `RegExp` — tested against `String(thrownValue)`.
  - a constructor/class — checked via `thrownValue instanceof error`.
  - a plain validator `Function` (not a constructor recognizable as a class)
    — called as `error(thrownValue)`; must return `true`.
  - a plain `object` or `Error` instance — every own enumerable property
    (plus non-enumerable `message`/`name`) on `error` must deep-strict-equal
    (or regex-match, if the property value is itself a `RegExp`) the
    corresponding property on the thrown value.
  - **Footgun (must be preserved as a runtime warning path in the .ts shim):**
    passing a plain `string` as `error` is silently reinterpreted as
    `message`, not as a validator — do not treat a bare string as an error
    matcher.
  `operator: "throws"`.

- **`assert.doesNotThrow(fn, error?, message?)`** — Calls `fn()`; throws
  `AssertionError` if `fn` throws an error matching `error` (RegExp or
  constructor only — no object/function-validator form here). If `fn` throws
  a *non-matching* error, that original error propagates uncaught (it is NOT
  swallowed). `operator: "doesNotThrow"`.

- **`assert.rejects(asyncFn, error?, message?)`** — `asyncFn` is either a
  function returning a `Promise` (called with no args) or a `Promise`
  directly. Awaits it; throws (as a rejected returned `Promise`)
  `AssertionError` if the promise fulfills instead of rejecting, or if the
  rejection reason doesn't match `error` (same matcher shapes as `throws`).
  Throws `ERR_INVALID_RETURN_VALUE` if `asyncFn` is a function that does not
  return a thenable. Returns `Promise<void>`. `operator: "rejects"`.

- **`assert.doesNotReject(asyncFn, error?, message?)`** — Mirror of
  `doesNotThrow` for promises: resolves if `asyncFn`'s promise fulfills or
  rejects with a non-matching reason (the non-matching rejection still
  propagates as the returned promise's rejection); rejects with
  `AssertionError` if it rejects with a *matching* reason. Returns
  `Promise<void>`. `operator: "doesNotReject"`.

- **`assert.fail(message?)`** — Always throws. `message` defaults to
  `"Failed"`. If `message` is an `Error` instance, that error is thrown
  as-is (not wrapped). Otherwise throws a new `AssertionError({ message,
  operator: "fail", stackStartFn: fail })`. Return type is `never`
  (documented as `void` by Node's own typings, but it never returns
  normally).

- **`assert.ifError(value)`** — No-op (`void`, returns) when `value` is
  `null` or `undefined`. For any other value (including `0`, `false`, `""`,
  which historically were also accepted — no longer, since v10.0.0), throws
  an `AssertionError` that **wraps** the original value as its `cause`-like
  payload: message is `"ifError got unwanted exception: " + inspect(value)`
  (or, if `value` is itself an `Error`, its own message is reused), and the
  thrown `AssertionError`'s stack is doctored to append the original error's
  stack for full traceability. `operator: "ifError"`.

### Properties & constants

- **`assert.strict`** — an object exposing every top-level function above,
  rebound so that `equal`/`notEqual`/`deepEqual`/`notDeepEqual` behave like
  their `*Strict*` siblings. Also has `assert.strict.strict === assert.strict`
  (idempotent self-reference, matching Node's own module shape) so
  `require("node:assert").strict.strict` keeps working.
- **`node:assert/strict`** — a distinct module specifier whose default export
  *is* `assert.strict` (same object identity as `require("node:assert").strict`
  in Node; RTS should preserve reference equality where its module system
  allows, or at minimum behavioral equality).
- **`assert.CallTracker`** — **removed** from Node's public API and its docs
  as of the v25 documentation snapshot (last documented as *Deprecated*,
  `DEP0558`, since ~v20.1.0/v18.17.0; fully absent from Node 25 docs at the
  time of writing). **Do not implement.** See §7.

### Events

None. `node:assert` has no `EventEmitter`-based objects.

## 3. Types & option objects

```ts
/** Options object accepted by `new assert.AssertionError(options)`. */
interface AssertionErrorOptions {
  /** Custom message; if omitted, one is auto-generated from actual/expected/operator. */
  message?: string;
  /** The actual value under test. */
  actual?: unknown;
  /** The expected value under test. */
  expected?: unknown;
  /** Assertion operator name, e.g. "strictEqual", "deepStrictEqual", "fail". */
  operator?: string;
  /** Function whose call-site is excluded from the generated stack trace. */
  stackStartFn?: (...args: any[]) => unknown;
  /** Diff rendering style used when auto-generating `message` for object/array diffs. */
  diff?: "simple" | "full";
}

/** Options accepted by `new assert.Assert(options)`. */
interface AssertOptions {
  /** Diff rendering style for auto-generated messages. Default: "simple". */
  diff?: "simple" | "full";
  /** When true, `equal`/`notEqual`/`deepEqual`/`notDeepEqual` behave strictly. Default: true. */
  strict?: boolean;
  /** When true, skip `[[Prototype]]`/constructor comparison in deep equality. Default: false. */
  skipPrototype?: boolean;
}

/** Matcher accepted by `throws`/`rejects` (full form) and `doesNotThrow`/`doesNotReject` (RegExp | Function only). */
type ErrorValidator =
  | RegExp
  | (new (...args: any[]) => Error)      // constructor form: instanceof check
  | ((thrown: unknown) => boolean)        // plain validator function
  | (Partial<Error> & Record<string, unknown>); // property-shape validator (throws/rejects only)

/** Shape produced internally by the deep-equal walker; not part of the public API but
 *  informs the RTS implementation's recursive comparator. */
interface DeepEqualMode {
  strict: boolean;          // Object.is + [[Prototype]] compare vs == + no prototype compare
  partial: boolean;         // only iterate `expected`'s own keys (partialDeepStrictEqual)
  skipPrototype: boolean;   // Assert{skipPrototype:true} — never compare [[Prototype]]/constructor
}
```

## 4. Node semantics & edge cases

- **Loose vs strict primitive comparison.** Legacy `equal`/`notEqual` use `==`
  /`!=` with one deviation from the real operator: `NaN` is treated as equal
  to `NaN` (real `==` says `NaN == NaN` is `false`). Strict forms use
  `Object.is`, under which `NaN` equals `NaN` and `+0`/`-0` are distinct
  (unlike `===`, which treats `+0 === -0`).
- **Deep-equal algorithm (legacy `deepEqual`/`notDeepEqual`):** primitives via
  `==` (NaN identical); object "type tag" (`Object.prototype.toString`-style
  internal class) must match; only **enumerable own** properties considered;
  object wrapper types (`new Number(1)` etc.) compared both as objects and via
  their unwrapped primitive value; `Error` `name`/`message`/`cause`/`errors`
  (AggregateError) are always compared regardless of enumerability; `Map`
  keys and `Set` items compared unordered; recursion **stops** at the first
  difference found or when a circular reference is re-encountered (does not
  throw on cycles — treats repeat visits as equal-so-far and stops
  descending); `[[Prototype]]` is **not** compared; `Symbol`-keyed properties
  are **not** compared; `WeakMap`/`WeakSet`/`Promise` are only equal to
  themselves (reference identity), never structurally.
- **Deep-strict-equal algorithm (`deepStrictEqual`/`notDeepStrictEqual`,
  always active regardless of mode):** primitives via `Object.is`; type tags
  must match; `[[Prototype]]` compared via `===` (unless
  `Assert{skipPrototype:true}`); only enumerable own properties (plus
  Symbol-keyed enumerable own properties, unlike legacy mode); `Error`
  `name`/`message`/`cause`/`errors` always compared including non-enumerable;
  object wrappers compared both ways like legacy; `Map`/`Set` unordered;
  cycle handling same as legacy (stop, don't throw); `RegExp` `source`,
  `flags`, and **`lastIndex`** are always compared (added v18.0.0); `Buffer`/
  `TypedArray` values compared by byte content *and* by concrete constructor
  (an `Int8Array` is never deep-strict-equal to a `Uint8Array` with the same
  bytes).
- **`partialDeepStrictEqual`:** subset of `deepStrictEqual` — only own
  enumerable (+ Symbol) properties present on `expected` are checked;
  `actual` may carry extra properties Node ignores; `[[Prototype]]` is never
  compared even without `skipPrototype`; sparse-array holes in `expected` are
  ignored (a hole never fails to match).
- **v25 behavior changes to note explicitly (must match, not the pre-v25
  behavior):** two distinct `Promise` instances are **never** considered deep
  equal even if both are e.g. already-resolved to the same value (was
  previously permissive in some paths); two `Date` objects that are both
  *invalid* (`NaN` internal time value) are now considered equal to each
  other (previously any comparison involving an invalid Date was `false`,
  including invalid-vs-invalid).
- **Circular reference handling:** since v24.0.0, recursion explicitly stops
  (rather than looping) upon re-encountering an object already on the current
  comparison path; this must be implemented with a visited-pairs guard
  (e.g. two parallel arrays/maps of already-compared `(actual, expected)`
  object pairs by identity), not a naive unbounded recursive walk.
- **Message semantics.** If the `message` argument is an `Error` instance, it
  is thrown directly (not wrapped in `AssertionError`, `generatedMessage` is
  irrelevant since no `AssertionError` is constructed at all). If it's a
  `string`, it becomes `AssertionError.message` verbatim and
  `generatedMessage` is `false`. If omitted, `generatedMessage` is `true` and
  the message is auto-built from `actual`/`expected`/`operator` using an
  inspect-style diff (`options.diff`: `"simple"` shows a compact diff;
  `"full"` shows the complete expected/actual object dumps).
- **Color / terminal output.** Node colorizes the generated diff when
  attached to a TTY; the environment variables `NO_COLOR` (any value) and
  `NODE_DISABLE_COLORS` (any value) both suppress color.
- **`stackStartFn`.** When set (internally, on every public assert function,
  passing the function itself), the generated `Error.stack` excludes frames
  at and below that function, so the reported stack trace points at the
  *caller's* call site, not into assert's own internals.
- **`assert.throws`/`assert.rejects` matcher pitfalls:** passing a bare
  `string` as the second argument is **always** interpreted as `message`,
  never as a matcher — a common user mistake Node explicitly documents as a
  footgun; RTS's `.ts` shim should preserve exactly this (mis-)behavior for
  compatibility, not "fix" it.
  `assert.doesNotThrow`/`assert.doesNotReject` support only `RegExp` or a
  constructor function for `error` — not the full object/validator-function
  form `throws`/`rejects` support.
- **`ifError` strictness (since v10.0.0):** any non-`null`/`undefined` value
  throws, including falsy values that were historically tolerated (`0`,
  `""`, `false`, `NaN`) — no exceptions.
- **Legacy mode deprecation status:** `assert.deepEqual`/`assert.equal`/
  `assert.notDeepEqual`/`assert.notEqual` are marked **Legacy** stability
  (changed from "Deprecated" back to "Legacy" in v16.0.0/v14.18.0) — kept
  indefinitely for compatibility, not scheduled for removal, but new code
  should use the `*Strict*` forms or `node:assert/strict`.
- **No platform (Windows vs POSIX) differences** — this module has zero
  filesystem/OS/network surface.
- **No errno-style codes** beyond the single `ERR_ASSERTION` code on
  `AssertionError`, plus `ERR_INVALID_RETURN_VALUE` thrown directly (not
  wrapped in `AssertionError`) when `assert.rejects`'s `asyncFn` function does
  not return a thenable.
- **Ordering guarantees:** none of these functions are async in the
  scheduling sense except `rejects`/`doesNotReject`, which simply `await` the
  given promise — no additional microtask reordering beyond native
  `await` semantics.
- **Backpressure:** not applicable (no streams).

## 5. RTS implementation notes

### 5.1 Native impl mapping

`node:assert` is almost entirely algorithmic (recursive value comparison +
error formatting) over values that are **already** first-class in the RTS
engine's value model (`PolyValue` primitives, primordial `Object`/`Array`,
primordial `TypedArray`/`ArrayBuffer`/`DataView`, primordial `Error`,
`RegExp`, `Symbol`, `BigInt`, `Proxy`/`Reflect`). There is **no OS/filesystem/
network surface** to map onto a Rust std module — unlike `fs`/`net`/`process`,
this module needs (at most) a thin native helper for stack-trace bookkeeping,
and otherwise is pure `.ts` shim logic built on primordial engine operations
(`typeof`, `===`, `Object.is`-equivalent intrinsic, `Object.keys`/
`getOwnPropertySymbols`, `Array`/`Map`/`Set` iteration, `Error.captureStackTrace`
if the engine exposes it, `util.inspect` for diff rendering — the last one is
a **same-crate** dependency on `node:util`, not on `rts-std`).

Concretely:
- Deep-equal walker (legacy/strict/partial/`skipPrototype`) → `.ts`,
  operating purely on primordial values via existing language operators
  (`typeof`, `Object.is` — expressible as `x === y || (x !== x && y !== y)`
  for the NaN case, or a native `Object.is` if the primordial `Object` class
  already exposes it), `Object.keys`, `Object.getOwnPropertySymbols`,
  `Array.isArray`, `.entries()`.
- `Buffer`/TypedArray content comparison → `.ts`, indexing the already
  primordial typed-array element accessors (no new native fn: element read
  is already engine-lowered per the primordial-TypedArray doctrine).
- Stack trace exclusion (`stackStartFn`) → reuses whatever native
  `Error`-capture hook already backs `Error().stack` / `Error.captureStackTrace`
  (primordial, implemented in `rts-primitives`, not owned by this module).
- Diff/message rendering → delegates to `node:util`'s `inspect` (also a
  rts-node module) for the `"full"` diff and a simpler custom string builder
  for `"simple"`.
- Color suppression → reads `NO_COLOR`/`NODE_DISABLE_COLORS` via
  `node:process`'s `env` (another rts-node module) — no direct native env
  read needed inside `assert` itself.

**Net result: no new Rust std module or external crate is required to back
`node:assert`.** This is the degenerate case for "native impl mapping" — the
mapping is "primordial engine value operations", already satisfied by the
engine + `rts-primitives`.

### 5.2 ABI surface

No new `extern "C"` symbols are required for the core assertion algorithm —
everything is expressible in `.ts` over primordial ops. Two **optional**
native helpers are worth exposing only if they materially simplify the `.ts`
shim (both are trivial and stateless; either can be deferred and inlined in
`.ts` instead, so treat both as "nice to have", not blocking):

| Symbol | Args (AbiType) | Return (AbiType) | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_ASSERT_SAME_VALUE` | `a: Handle/PolyValue passthrough` (via generic call ABI, not a typed extern — `Object.is` on two `any`s is not expressible as a narrow `AbiType` pair) | `Bool` | Optional; only useful if the primordial `Object` class does not already expose an `Object.is`-equivalent the `.ts` shim can call directly. Prefer reusing an existing primordial `Object.is` if present — do not add this if so. |
| `__RTS_FN_NODE_ASSERT_TYPE_TAG` | `value` (generic) | `StrPtr` | Optional; returns the internal `[Symbol.toStringTag]`/`Object.prototype.toString` class tag used by the deep-equal type-tag check. Only needed if this isn't already exposed to `.ts` via an existing primordial `Object.prototype.toString.call(x)` path — prefer that path and skip this extern entirely. |

No object in this module's surface becomes an opaque `Handle`: `AssertionError`
and `Assert` instances are ordinary primordial `Error`/`Object`-shaped values
constructed entirely in `.ts`, not Rust-side resources needing a `HandleTable`
slot. **This module's entire implementation is a `.ts` shim** — see §5.6.

### 5.3 Async model

Only `assert.rejects`/`assert.doesNotReject` touch async at all, and they do
so purely by `await`-ing a promise the caller already produced — there is
**no new scheduling, no callback-style variant, and no tokio dependency**.
The `.ts` shim implements them as:

```ts
async function rejects(asyncFn: (() => Promise<any>) | Promise<any>, error?, message?) {
  const p = typeof asyncFn === "function" ? asyncFn() : asyncFn;
  if (!(p instanceof Promise)) throw new TypeError(/* ERR_INVALID_RETURN_VALUE */);
  try { await p; } catch (e) { /* validate e against `error`, else return */ return; }
  throw new AssertionError({ message, operator: "rejects" });
}
```

This relies exclusively on the engine's already-primordial `Promise`/`await`
machinery (see `docs/specs/async-promise-function.md`) — no `rts-std`
`async_rt`/tokio dependency, since the module never spawns work itself, it
only observes a promise's settlement.

### 5.4 Multithread / worker interaction

`node:assert` carries **no mutable module-level state**. `assert.strict` is
an immutable rebound view created once at module-init; `Assert` instances are
plain user-owned objects (their `diff`/`strict`/`skipPrototype` config lives
in the instance itself, in normal heap memory, like any other user object).
Under the RTS threading model (`docs/specs/rts-threading-model.md`), this
means:
- No `threadLocal`/`shared` distinction is needed inside the module — it has
  no singleton mutable resource comparable to a `fs` file-descriptor table or
  a `net` connection registry.
- An `Assert` instance created in one worker and passed across a
  `MessagePort`/`SharedArrayBuffer` boundary is just a plain object; if the
  RTS structured-clone path (used by `worker_threads.postMessage`) can clone
  arbitrary primordial objects, an `Assert` instance clones like any other
  object (its methods, being closures, will need the same closure-cloning
  story as any other cross-worker function value — no assert-specific
  handling required).
- Nothing in this module needs promotion-on-publication semantics beyond what
  already applies to any plain object crossing a worker boundary.

### 5.5 Buffer / TypedArray interop

Deep-equality comparisons that reach a `Buffer`/`TypedArray`/`ArrayBuffer`/
`DataView` operand compare:
1. concrete constructor identity (an `Int8Array` is never equal to a
   `Uint8Array` of identical bytes),
2. `byteLength`,
3. element-by-element content.

Because `Buffer`/TypedArrays are primordial engine-owned memory (raw
`ArrayBuffer` + element-indexing already lowered by the engine per the
primordial doctrine), this comparison never needs to cross the native ABI
boundary at all — it is pure `.ts` iteration over already-addressable engine
values, exactly like comparing a plain `Array`. There is no
marshalling/copy step: `assert` never takes ownership of or mutates buffer
contents, it only reads.

### 5.6 Doctrine placement

`node:assert` is **non-primordial** (no native literal/syntactic form — you
reach it only via `import ... from "node:assert"`). Per the
primordial-vs-registry doctrine, the engine must not hardcode the string
`"assert"` anywhere in `crates/rts-codegen-new/`. Resolution flow:

1. `import assert from "node:assert"` (or `"node:assert/strict"`) is handled
   by the **generic** node-module-import mechanism shared by every `node:*`
   specifier — a data lookup (`NODE_SPECS` / `node_lookup` / `ns_prefix_for`
   in `rts-node`) mapping the module name `"assert"` (and the alias
   `"assert/strict"`) to its `.ts` shim file + (if any) native extern
   namespace prefix, e.g. `node_assert`.
2. Since §5.2 concludes essentially zero native externs are required, the
   `NODE_SPECS` row for `"assert"` may carry an **empty or near-empty**
   native member table — this is expected and fine; the data table's job is
   to route the import to the `.ts` shim, not to guarantee a nonzero native
   surface.
3. The `.ts` shim (shipped by `rts-node`, not `rts-primitives`/`rts-shared`,
   since it's Node-specific, not a universal JS global) implements 100% of
   the classes and functions in §2 directly in TypeScript, using only
   primordial engine operations plus (optionally) calls into the `node:util`
   shim from the same crate for inspect-based diff rendering.
4. No `.ts` prelude injects an `__rts_wk_*` hook specific to `assert` — the
   module needs no engine-side cooperation beyond what already exists for
   `Error`/`Object`/`Array`/`TypedArray`/`Promise`/`RegExp`.

### 5.7 Shared-infra dependencies (FLAG)

None from `rts-std`. `node:assert` needs no event loop, no tokio runtime, no
TLS/crypto primitives, and no net sockets — it is pure synchronous value
comparison plus a bare `await` on a caller-supplied `Promise` (already
primordial, already usable without `rts-std`). The only cross-module
dependencies are same-crate (`rts-node`) conveniences, not shared-infra
hoists:

- **`node:util`'s `inspect`** (same `rts-node` crate) — used for `"full"`-mode
  diff rendering in auto-generated `AssertionError` messages; not a hard
  blocker (a minimal own string-dump can stand in until `node:util` lands),
  but implementing `node:assert`'s diff output after or alongside `node:util`
  avoids rework.
- **`node:process`'s `env`** (same `rts-node` crate) — used only to read
  `NO_COLOR`/`NODE_DISABLE_COLORS` for color suppression; trivial, not
  blocking.

### 5.8 Implementation phases

a. Scaffold the `NODE_SPECS` row for `"assert"` (+ alias `"assert/strict"`)
   pointing at a new `.ts` shim file; stub every export in §2 with
   `todo!()`-equivalent (`throw new Error("not implemented")`) bodies so the
   import resolves and the full named-export surface type-checks.
b. Implement `AssertionError` (constructor + all 6 documented properties) and
   the truthiness/equality primitives: `ok`/`assert`, `equal`, `notEqual`,
   `strictEqual`, `notStrictEqual` (legacy `==`/`NaN`-as-equal semantics for
   the non-strict pair; `Object.is`-based for the strict pair).
c. Implement the shared recursive deep-equal walker parameterized by
   `{strict, partial, skipPrototype}` (§3 `DeepEqualMode`) — covering
   primitives, plain objects, arrays (incl. sparse-hole handling for the
   partial mode), `Map`/`Set` (unordered), `Error`/`AggregateError`
   (name/message/cause/errors always compared), object wrappers
   (Number/String/Boolean boxed), `RegExp` (source/flags/lastIndex),
   `Buffer`/TypedArray/`ArrayBuffer`/`DataView` (constructor+bytes), `Date`
   (incl. v25's invalid-Date-equals-invalid-Date rule), `Promise`/`WeakMap`/
   `WeakSet` (reference-identity only), and circular-reference short-
   circuiting via a visited-pairs list. Wire `deepEqual`/`notDeepEqual`/
   `deepStrictEqual`/`notDeepStrictEqual`/`partialDeepStrictEqual` onto it.
d. Implement `match`/`doesNotMatch`.
e. Implement `throws`/`doesNotThrow` with all matcher shapes (RegExp /
   constructor / validator function / property-shape object for `throws`;
   RegExp / constructor only for `doesNotThrow`), including the
   string-as-message footgun behavior.
f. Implement `rejects`/`doesNotReject` as thin async wrappers reusing (d)'s
   matcher logic against the rejection reason.
g. Implement `fail`/`ifError` (including `ifError`'s stack-augmentation
   wrapping behavior).
h. Implement `assert.strict` (rebind non-strict-named functions to strict
   behavior, `strict.strict === strict`) and the `"node:assert/strict"`
   module specifier as an alias resolving to the same object.
i. Implement the `Assert` class wrapping (b)-(h) with per-instance
   `diff`/`strict`/`skipPrototype`, taking care that each instance method is
   a closure bound to that instance's config (so destructured methods keep
   working per the documented caveat, or at minimum documented as RTS's
   chosen deviation if closures-over-`this` prove awkward in the `.ts`
   shim's codegen path).
j. Implement diff/message auto-generation (`"simple"` vs `"full"`), wired to
   `node:util.inspect` when available, with `NO_COLOR`/`NODE_DISABLE_COLORS`
   suppression via `node:process.env`.
k. Register the module's public `.d.ts` surface (all §2 exports) and confirm
   `node:assert`/`node:assert/strict` both resolve through the generic
   node-module import path with zero engine-side hardcoding.
l. Land the test fixtures in §6.

## 6. Test plan

- `tests/node/assert/ok_and_equal.test.ts` — `assert(true)`/`assert.ok(1)`
  pass; `assert(false)`/`assert.ok(0)`/`assert.ok("")` throw
  `AssertionError` with `code === "ERR_ASSERTION"`; `assert.equal(1, "1")`
  passes (legacy coercion) while `assert.strictEqual(1, "1")` throws.
- `tests/node/assert/nan_and_zero.test.ts` — `assert.strictEqual(NaN, NaN)`
  passes; `assert.strictEqual(0, -0)` throws; `assert.equal(NaN, NaN)`
  passes (legacy NaN-as-equal quirk); `assert.notStrictEqual(0, -0)` passes.
- `tests/node/assert/deep_equal_legacy_vs_strict.test.ts` — object with
  extra `[[Prototype]]` differences: `deepEqual` passes, `deepStrictEqual`
  throws; boxed `new Number(1)` vs raw `1`: legacy `deepEqual` semantics vs
  strict; Symbol-keyed property present only in strict comparison.
- `tests/node/assert/deep_equal_collections.test.ts` — `Map`/`Set` compared
  unordered; nested `Map` of `Array` of `Object`; circular object compares
  equal to itself without stack overflow (self-referential `a.self = a`).
- `tests/node/assert/deep_equal_typed_arrays.test.ts` — `Int8Array` vs
  `Uint8Array` with identical bytes → not equal; identical `Int8Array`
  content → equal; `ArrayBuffer`/`DataView` byte comparison.
- `tests/node/assert/deep_equal_errors.test.ts` — two `Error` instances with
  same `message`/`name` but different (non-enumerable) extra props → equal
  under `deepStrictEqual` if only compared fields match; `AggregateError`
  `.errors` array compared.
- `tests/node/assert/partial_deep_strict_equal.test.ts` — `expected` is a
  subset of `actual`'s keys → passes; extra key present only in `expected`
  but missing/different in `actual` → throws; `[[Prototype]]` mismatch does
  NOT fail (unlike `deepStrictEqual`).
- `tests/node/assert/match.test.ts` — `assert.match("abc", /b/)` passes;
  `assert.doesNotMatch("abc", /z/)` passes; non-string input throws for
  both.
- `tests/node/assert/throws_matchers.test.ts` — four variants: RegExp
  matcher against `error.message`; class/constructor matcher via
  `instanceof`; validator function matcher returning `true`/`false`; plain
  object matcher with a `RegExp` property value. Also: `fn` that does not
  throw → `AssertionError`; passing a bare string as the second arg is
  treated as `message`, not a matcher (assert with a throwing fn and a
  string second-arg where the thrown value would NOT satisfy a real
  matcher — must still pass since the string is just the failure message).
- `tests/node/assert/does_not_throw.test.ts` — non-matching thrown error
  propagates uncaught through `doesNotThrow` (test via try/catch around the
  `doesNotThrow` call site expecting the *original* error, not
  `AssertionError`).
- `tests/node/assert/rejects_resolves.test.ts` — `await
  assert.rejects(Promise.reject(new TypeError("x")), TypeError)` passes;
  `await assert.rejects(Promise.resolve(1))` throws; `await
  assert.rejects(() => 42 as any)` throws `ERR_INVALID_RETURN_VALUE` (fn
  didn't return a promise).
- `tests/node/assert/does_not_reject.test.ts` — mirrors `doesNotThrow` for
  the async case, including the "non-matching rejection still propagates"
  behavior.
- `tests/node/assert/fail_and_iferror.test.ts` — `assert.fail()` message
  defaults to `"Failed"`; `assert.fail(new TypeError("x"))` throws that
  exact `TypeError` instance (not wrapped); `assert.ifError(null)`/
  `assert.ifError(undefined)` pass; `assert.ifError(0)`/`assert.ifError("")`
  throw (post-v10 strictness); `assert.ifError(new Error("boom"))` throws an
  `AssertionError` whose message embeds `"boom"`.
- `tests/node/assert/strict_mode.test.ts` — `import assert from
  "node:assert/strict"` then `assert.equal(1, "1")` throws (behaves like
  `strictEqual`); `require("node:assert").strict.strict ===
  require("node:assert").strict`.
- `tests/node/assert/assert_class.test.ts` — `new Assert({diff:"full"})`
  produces a distinct instance whose `deepStrictEqual` still throws on a
  real mismatch; `new Assert({strict:false}).equal(1,"1")` passes (loose);
  `new Assert({skipPrototype:true}).deepStrictEqual(new Foo(1), new Bar(1))`
  passes when `Foo`/`Bar` are structurally-identical differently-named
  classes.
- `tests/node/assert/custom_message.test.ts` — passing a custom string
  `message` sets `generatedMessage: false`; omitting it sets
  `generatedMessage: true` and produces a non-empty diff-derived message;
  passing an `Error` instance as `message` throws that exact instance.
- `tests/node/assert/worker_thread_assert.test.ts` *(multithread)* —
  spawn a `worker_threads` Worker that runs a battery of the above
  assertions inside the worker and posts back pass/fail counts; confirms no
  shared-state corruption between the main thread's and the worker's
  independent `Assert`/`assert.strict` usage (validates §5.4's "no
  module-level mutable state" claim under real concurrent use).

## 7. Open questions / deferrals

- **`assert.CallTracker`** was deprecated (`DEP0558`, ~v20.1.0/v18.17.0) and
  is absent from the Node 25 documentation entirely at the time of writing —
  **not implemented** by this spec. If a future Node LTS reintroduces or a
  user dependency still requires it, revisit as a separate follow-up rather
  than folding it into this P0 pass.
- **Diff-rendering pixel/text parity** with Node's exact `"simple"`/`"full"`
  diff output (colors, alignment, truncation of very large objects) is
  explicitly **best-effort** — functional parity (correct pass/fail
  semantics, a readable message) is required now; byte-for-byte output
  parity with Node's own formatter is deferred until `node:util.inspect`
  itself reaches high fidelity.
- **`Object.is`/type-tag native helpers** (§5.2's two optional externs) — do
  not add either unless implementing the `.ts` shim purely in terms of
  existing primordial operators/`Object`-class methods proves impractical;
  default assumption is that no new native surface is needed at all.
- **`BigInt`/`Symbol` operands in deep-equal** should fall straight out of
  the existing primordial `BigInt`/`Symbol` value model (tag compare +
  `Object.is`-equivalent), but full cross-runtime verification is deferred
  until BigInt/Symbol reach broader parity elsewhere in the engine (per the
  project's measured-cluster prioritization) — flag any gap found during
  §6 test authoring as a follow-up issue rather than blocking this module.
- **Structured-clone of `Assert` instances across `worker_threads`
  boundaries** (§5.4) depends on the engine's general function/closure
  cross-worker cloning story, which is itself still evolving under the RTS
  threading model — the `worker_thread_assert.test.ts` fixture is written to
  avoid depending on that (each worker constructs its own local `Assert`
  instance rather than receiving one via `postMessage`); revisit once
  closure-cloning-across-workers has its own spec.
