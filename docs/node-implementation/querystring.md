# node:querystring

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:querystring` |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P1 |
| Status | [ ] Not implemented — spec only |
| Import forms | `import querystring from 'node:querystring'`; `import { parse, stringify, escape, unescape, decode, encode } from 'node:querystring'`; `const querystring = require('node:querystring')` |
| Globals exposed | none (all access is via the `node:querystring` module import; no ambient globals) |

## 1. Purpose

`node:querystring` provides utilities for parsing and formatting URL query strings — the substring after the `?` in a URL, e.g. `foo=bar&abc=xyz&abc=123`. It is a **legacy, non-standardized** sibling to the standard `URLSearchParams` class (`node:url`): `querystring` is more performant (no full WHATWG URL spec compliance overhead, no iterator protocol, no percent-decoding normalization beyond the module's own rules) but does not follow any web standard, so its parsing/encoding edge cases can diverge from what a browser or `URLSearchParams` would produce. Node's own guidance: use `URLSearchParams` when performance is not critical or browser-code compatibility is desired; use `querystring` when raw throughput on query-string (de)serialization matters and the legacy semantics (arrays for repeated keys, prototype-less parse result, custom separator/equals characters) are acceptable or desired.

The module has exactly six top-level functions, no classes, no events, and (per the fetched Node 25 documentation) no publicly documented properties/constants — it is one of the smallest namespaces in Node's core API surface.

## 2. Exported API surface (COMPLETE)

### Classes

None. `node:querystring` exports no classes and defines no constructors.

### Top-level functions

| Function | Variant |
|---|---|
| `querystring.decode(str[, sep[, eq[, options]]])` | sync |
| `querystring.encode(obj[, sep[, eq[, options]]])` | sync |
| `querystring.escape(str)` | sync |
| `querystring.parse(str[, sep[, eq[, options]]])` | sync |
| `querystring.stringify(obj[, sep[, eq[, options]]])` | sync |
| `querystring.unescape(str)` | sync |

All six functions are synchronous, pure (no I/O, no shared mutable module state beyond the overridable `escape`/`unescape` function properties described below), and have been stable since Node v0.1.x (`escape`/`unescape`/`parse`/`stringify` since v0.1.25; `decode`/`encode` since v0.1.99). None are deprecated in Node 25.

#### `querystring.decode(str[, sep[, eq[, options]]])`

Alias for `querystring.parse()` — identical signature, identical behavior, identical return shape. Provided for legacy code that used the pre-`parse`/`stringify` naming.

| Name | Type | Optional | Default |
|---|---|---|---|
| `str` | `string` | no | — |
| `sep` | `string` | yes | `'&'` |
| `eq` | `string` | yes | `'='` |
| `options` | `ParseOptions` | yes | `{}` |

Return type: `Record<string, string \| string[]>` (prototype-less object — see §4). Throws: none documented (malformed input degrades gracefully — see §4). Variant: sync.

#### `querystring.encode(obj[, sep[, eq[, options]]])`

Alias for `querystring.stringify()` — identical signature, identical behavior, identical return value.

| Name | Type | Optional | Default |
|---|---|---|---|
| `obj` | `Record<string, unknown>` | no | — |
| `sep` | `string` | yes | `'&'` |
| `eq` | `string` | yes | `'='` |
| `options` | `StringifyOptions` | yes | `{}` |

Return type: `string`. Throws: none documented. Variant: sync.

#### `querystring.escape(str)`

Percent-encodes a string, optimized for URL query-string requirements (a purpose-built subset/variant of what `encodeURIComponent` does — Node implements its own routine rather than delegating, for performance). Used internally as the default `encodeURIComponent` option of `stringify()`.

| Name | Type | Optional | Default |
|---|---|---|---|
| `str` | `string` | no | — |

Return type: `string`. Throws: none documented (non-string input is coerced). Variant: sync. Note: exported specifically so application code MAY replace it (`querystring.escape = myImpl`) to swap the encoding implementation used by `stringify()`'s default; not expected to be called directly by typical application code, but must remain a first-class, independently-callable, independently-overridable export.

#### `querystring.parse(str[, sep[, eq[, options]]])`

Parses a URL query string into a collection of key/value pairs.

| Name | Type | Optional | Default |
|---|---|---|---|
| `str` | `string` | no | — |
| `sep` | `string` | yes | `'&'` — substring delimiting key/value **pairs** |
| `eq` | `string` | yes | `'='` — substring delimiting a pair's **key and value**; may have length > 1 (since v6.0.0 / v4.2.4) |
| `options` | `ParseOptions` | yes | `{}` |
| `options.decodeURIComponent` | `(encodedURIComponent: string) => string` | yes | `querystring.unescape` |
| `options.maxKeys` | `number` | yes | `1000` — pass `0` to remove the limit entirely |

Return type: `Record<string, string \| string[]>`. Throws: none documented — malformed/incomplete pairs and encoding errors degrade gracefully rather than throwing (see §4). Variant: sync.

Behavioral notes baked into the signature:
- Repeated keys accumulate into a `string[]` in first-seen order; a key seen once stays a plain `string` (this is why the return type is a union, not always an array — matches Node's documented example `{ foo: 'bar', abc: ['xyz','123'] }`).
- The returned object does **not** prototypically inherit from `Object.prototype` (since v6.0.0) — no `.toString()`, `.hasOwnProperty()`, `.constructor`, etc. are present on it; it behaves like an object created with `Object.create(null)`.
- Since v8.0.0, multiple empty entries parse correctly (e.g. `&=&=` produces two empty-string key/value pairs rather than being collapsed/dropped).

#### `querystring.stringify(obj[, sep[, eq[, options]]])`

Serializes an object into a URL query string, iterating only the object's own enumerable properties.

| Name | Type | Optional | Default |
|---|---|---|---|
| `obj` | `Record<string, unknown>` | no | — |
| `sep` | `string` | yes | `'&'` |
| `eq` | `string` | yes | `'='` |
| `options` | `StringifyOptions` | yes | `{}` |
| `options.encodeURIComponent` | `(str: string) => string` | yes | `querystring.escape` |

Return type: `string`. Throws: none documented. Variant: sync.

Supported value types per property: `string | number | bigint | boolean | string[] | number[] | bigint[] | boolean[]`. Numeric values must be finite (non-finite — `NaN`/`Infinity`/`-Infinity` — are **not** validated by a thrown error in the documented surface; treat as an implementation-defined edge to verify against real Node, see §4/§7). Any other value type is coerced to an empty string for that key's value. An array value produces one `key=value` pair per array element, all under the same key, in array order (`{ baz: ['qux','quux'] }` → `baz=qux&baz=quux`).

#### `querystring.unescape(str)`

Decodes a percent-encoded string. Uses the built-in `decodeURIComponent()` algorithm by default; if `decodeURIComponent()` throws on malformed input, `unescape()` falls back to a safer, non-throwing equivalent decoding routine (so `unescape()` itself is documented as non-throwing even on malformed percent-escapes, unlike raw `decodeURIComponent`).

| Name | Type | Optional | Default |
|---|---|---|---|
| `str` | `string` | no | — |

Return type: `string`. Throws: none documented (the fallback path exists precisely to avoid `URIError` propagating out of `unescape`). Variant: sync. Note: exported specifically so application code MAY replace it (`querystring.unescape = myImpl`) to swap the decoding implementation used by `parse()`'s default.

### Properties & constants

None documented for this module in Node 25. `querystring.escape` and `querystring.unescape` are themselves overridable function *properties* of the module namespace object (see notes above) — they are listed under Top-level functions since that is their primary role, but implementers must expose them as ordinary mutable properties of the `node:querystring` namespace object (assignable, not frozen), because overriding them is the module's documented extension mechanism for custom character encodings (e.g. GBK).

### Events

None. `node:querystring` defines no `EventEmitter`-based objects; every export is a plain synchronous function.

## 3. Types & option objects

```typescript
interface ParseOptions {
  /**
   * The function to use when decoding percent-encoded characters in the
   * query string. Default: querystring.unescape().
   */
  decodeURIComponent?: (encodedURIComponent: string) => string;
  /**
   * Specifies the maximum number of keys to parse. Specify 0 to remove
   * key-counting limitations. Default: 1000.
   */
  maxKeys?: number;
}

interface StringifyOptions {
  /**
   * The function to use when converting URL-unsafe characters to
   * percent-encoding in the query string. Default: querystring.escape().
   */
  encodeURIComponent?: (str: string) => string;
}

/**
 * Return shape of parse()/decode(). NOT a plain Object: it does not inherit
 * from Object.prototype (no toString/hasOwnProperty/valueOf/constructor).
 * A key seen exactly once maps to a string; a key seen 2+ times maps to a
 * string[] in first-seen order.
 */
type ParsedQuery = Record<string, string | string[]>;

/**
 * Values that stringify()/encode() accept as an obj's property value.
 * Anything else coerces to '' for that key.
 */
type StringifiableValue =
  | string | number | bigint | boolean
  | string[] | number[] | bigint[] | boolean[];

type StringifiableObject = Record<string, StringifiableValue | unknown>;
```

No other option objects, return shapes, or callback signatures exist in this module — every function is synchronous, taking primitive/plain-object arguments and returning a primitive/plain object directly (no callbacks, no promises, no error-first convention).

## 4. Node semantics & edge cases

- **Not a standardized API.** Unlike `URLSearchParams` (WHATWG URL spec), `querystring`'s parsing/encoding rules are Node-specific historical behavior. RTS must match *Node's* behavior byte-for-byte, not the WHATWG spec's — the two differ on details like `+` handling, repeated-key aggregation, and array serialization.
- **`parse()` never throws on malformed input.** Missing `=`, an empty segment (`a&&b=1`), a trailing separator, or an unpaired `eq` all degrade to producing an empty-string value or skipping the entry, rather than throwing. `decodeURIComponent` inside `parse` catches malformed percent-escapes per Node's default `unescape` (falls back to a non-throwing decode instead of raising `URIError`) — a custom `options.decodeURIComponent` supplied by the caller that itself throws will propagate that throw uncaught (Node does not wrap caller-supplied decode functions in a try/catch).
- **`eq` may be multi-character** (since Node v6.0.0 / v4.2.4 backport) — e.g. `eq: '::'` for `a::1;b::2`. Both `sep` and `eq` are treated as literal substrings, not regexes; if `sep`/`eq` contain regex metacharacters they are still matched literally (Node does simple substring scanning, not regex compilation) — RTS's native implementation must not accidentally treat them as patterns.
- **`maxKeys` truncates, it does not error.** Once `maxKeys` (default 1000) distinct pairs have been parsed, `parse()` silently stops accumulating further pairs (no exception, no truncation flag on the result). `maxKeys: 0` disables the limit entirely (parses everything, unbounded). This is a potential DoS vector Node explicitly does **not** protect against by default beyond the 1000-key default — an untrusted, extremely long query string with `maxKeys: 0` can be used to exhaust memory; RTS should preserve Node's default of 1000 exactly (not silently pick a different default) since this is a documented security-relevant knob.
- **Repeated-key aggregation order.** Values for a repeated key accumulate in **first-seen order** as they appear left-to-right in the input string, not sorted, not last-wins. A key that appears exactly once yields a bare `string`, not a single-element array — callers must be prepared for the return type of any given key to be `string | string[]` and typically write `[].concat(result[key])` to normalize, exactly as Node's own docs/ecosystem code does.
- **Return value does not inherit `Object.prototype`** (since v6.0.0). This is a deliberate prototype-pollution hardening measure: query strings are attacker-controlled in many web-server contexts, and a key like `__proto__`, `constructor`, or `toString` must land as an **own data property** on the result, never mutate/shadow `Object.prototype` methods or trigger prototype getter/setter chains. RTS's native implementation must construct the result as a null-prototype object (Rust-side equivalent: build a fresh JS object via the engine's object/shape machinery with no prototype link, analogous to `Object.create(null)`), not merely "an object that happens not to have inherited props checked."
- **`stringify()` iterates only own enumerable properties** of `obj` — inherited/prototype properties are not serialized, `Symbol`-keyed properties are not serialized (query strings are string-keyed only), non-enumerable properties are skipped.
- **`stringify()` value coercion.** `string | number | bigint | boolean` (and arrays thereof) serialize their natural string form (`String(value)`); numeric values must be finite — Node's behavior for `NaN`/`±Infinity` is to still call `String()` on them (`"NaN"`, `"Infinity"`, `"-Infinity"`), which is **not URL-decodable back to the same number** — this is a real semantic edge Node does not special-case, and RTS should replicate rather than "fix" (verify exact behavior empirically against a real Node 25 binary before finalizing — flagged in §7). Any value that is not one of the supported types (e.g. a plain object, `undefined`, `null`, `Symbol`) coerces to the empty string `''` for that key.
- **Array values serialize as repeated `key=value` pairs**, not as `key[]=value` or a single joined string — `{ list: [1,2,3] }` → `list=1&list=2&list=3`, matching how `parse()` re-aggregates them back into an array (`parse(stringify(obj))` round-trips for this shape).
- **`escape()`/`unescape()` are user-overridable module properties**, not just documented defaults — application code is documented as being allowed to reassign `querystring.escape = customFn` / `querystring.unescape = customFn` to change the encoding used by `stringify()`/`parse()`'s *own* defaults (e.g. supporting GBK-encoded query strings via a custom `decodeURIComponent`/`encodeURIComponent` option, which is the documented mechanism — but `escape`/`unescape` themselves being reassignable is also documented as the lower-level override point). RTS's `.ts` shim must expose these as ordinary mutable exports (not `Object.freeze`d, not native-only constants), and `stringify`/`parse`'s native or shim implementation must read the *current* value of `querystring.escape`/`querystring.unescape` at call time (late binding), not a value captured once at module load.
- **UTF-8 by default; alternate encodings via the options callbacks.** Both `parse` and `stringify` assume UTF-8 unless the caller supplies a custom `decodeURIComponent`/`encodeURIComponent` (Node's own doc example uses a GBK codec). RTS's native `escape`/`unescape`/default paths must operate on UTF-8 byte sequences consistent with the rest of the engine's string representation.
- **No platform (Windows vs POSIX) differences** — this module does no filesystem, process, or OS-facility interaction; behavior is purely string-algorithmic and platform-independent.
- **No errno/error-code surface** — no function in this module is documented to throw a Node-style `Error` with a `.code`; robustness against malformed input is handled by graceful degradation (empty string / entry-skipping) rather than exceptions. RTS should not invent thrown errors where Node has none — a native implementation that panics/throws on malformed query strings would be a parity regression.
- **No backpressure/streaming concerns** — every function takes a whole string/object and returns a whole string/object; there is no chunked or streaming variant.
- **No deprecations** in Node 25 — all six functions remain fully supported; `decode`/`encode` are long-standing aliases, not soft-deprecated forms (unlike, say, `url.parse()` elsewhere in Node).
- **Security note (prototype pollution).** The null-prototype return value of `parse()`/`decode()` (see above) is itself the documented security hardening for this exact module — RTS must not "simplify" this away by returning a normal object literal, or a key like `__proto__` in attacker-supplied query input becomes an own property that downstream code might mistake for (or be confused with) the actual prototype-chain `__proto__` accessor, opening exactly the class of prototype-pollution bug this hardening exists to prevent.

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node` is a fully independent crate (no `rts-std` dependency). `node:querystring` needs **no OS/IO/async backend at all** — it is pure string algorithms, so the "native impl" is simply hand-written Rust string-processing code inside `rts-node` itself, not a wrapped external crate or OS facility:

- **`escape`/`unescape` core encode/decode.** A small percent-encoding/decoding routine in `rts-node/src/querystring/codec.rs`, operating on UTF-8 `&str`/byte slices. `escape` mirrors Node's own C++/JS `querystring` escape table (a specific "URL query string safe" unreserved-character set — distinct from, and slightly different from, the generic `encodeURIComponent` unreserved set; must be verified byte-for-byte against Node's actual escape table, see §7). `unescape` mirrors `decodeURIComponent`'s percent-decoding with the documented non-throwing fallback (on a malformed `%XY` escape, pass the raw bytes through unchanged rather than raising).
- **`parse`/`decode` core.** A single-pass scanner in `rts-node/src/querystring/parse.rs`: split on `sep` (substring search, not regex), then each pair split on the **first** occurrence of `eq` (substring search), apply the decode function to both key and value, and aggregate into an ordered map that preserves first-seen-key order and repeated-key array promotion, capped at `maxKeys` (0 = unbounded). The result is handed to the engine's object/shape layer to construct a **null-prototype** object (no `Object.prototype` link) — this is a property-shape concern, not a string-algorithm concern, and must route through however RTS constructs `Object.create(null)`-shaped objects today (shape with `proto_id = 0`/none), not a hand-rolled `HashMap` exposed with a fake prototype.
- **`stringify`/`encode` core.** A single-pass serializer in `rts-node/src/querystring/stringify.rs`: iterate the input object's **own enumerable string-keyed properties** (via the engine's object-property-enumeration primitive — this must reuse whatever primitive `Object.keys`/`for...in` already use to get "own enumerable" semantics right, not reinvent enumeration), coerce each value per the type-dispatch rules in §4, apply the encode function to key and value, join with `eq`, join pairs with `sep`.
- All four cores are pure functions over UTF-8 strings/engine objects — no `Mutex`/`OnceLock`/shared state, **except** the overridable `escape`/`unescape` function slots (see 5.2/5.6), which need a small piece of *mutable* per-module state (a function-pointer/callable slot the `.ts` shim can reassign) since Node documents them as reassignable properties.

### 5.2 ABI surface

Symbol convention: `__RTS_FN_NODE_QUERYSTRING_<NAME>`. This module needs **no `Handle`-based objects at all** — every value crossing the ABI is a `StrPtr` (UTF-8 string) or a primitive number; there is no stateful/rich object to slab-allocate. The `options` objects (`decodeURIComponent`/`encodeURIComponent`/`maxKeys`) and the array-vs-string aggregation logic for `parse`/`stringify` are handled by the `.ts` shim, which calls into a small set of native primitives for the actual scanning/codec work and constructs/enumerates the JS object shape itself using existing engine object primitives (not a querystring-specific object ABI).

| Symbol | Args (AbiType) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_QUERYSTRING_ESCAPE` | `StrPtr str` | `StrPtr` | pure; used by default `stringify` path and directly by `querystring.escape()` |
| `__RTS_FN_NODE_QUERYSTRING_UNESCAPE` | `StrPtr str` | `StrPtr` | pure; never errors (graceful fallback baked into the native routine itself) |
| `__RTS_FN_NODE_QUERYSTRING_SPLIT_PAIRS` | `StrPtr str, StrPtr sep, StrPtr eq, I32 maxKeys` | `StrPtr` (JSON-encoded `[[key, value], ...]` in first-seen order, pre-percent-decoding) | the substring-splitting/pairing scan only; percent-decoding of each key/value is applied by the `.ts` shim so a caller-supplied custom `decodeURIComponent` (a JS function) can be invoked per-pair without the native layer needing to call back into JS for the default case |
| `__RTS_FN_NODE_QUERYSTRING_JOIN_PAIRS` | `StrPtr pairsJson (JSON [[key, value], ...], already percent-encoded), StrPtr sep, StrPtr eq` | `StrPtr` | the substring-joining half of `stringify`; percent-encoding of each key/value happens in the `.ts` shim before calling this (again so a custom `encodeURIComponent` JS callback needs no native callback path) |

Design rationale for the split (`SPLIT_PAIRS`/`JOIN_PAIRS` vs. one monolithic native `parse`/`stringify`): the **default** path (no custom `decodeURIComponent`/`encodeURIComponent`) should still be fully native and fast (this module's entire value proposition vs. `URLSearchParams` is raw performance) — so the `.ts` shim's fast path calls `ESCAPE`/`UNESCAPE` natively per key/value (still native calls, just from `.ts`, no JS-level string algorithm) and only falls back to invoking a **user-supplied JS function** per key/value when `options.decodeURIComponent`/`options.encodeURIComponent` is explicitly passed — which is unavoidably a JS callback since the whole point of the option is letting *user JS code* supply the codec. This keeps the common case 100% native-code string processing while still supporting the documented override mechanism without inventing a native-to-JS callback ABI for it.

Building the null-prototype result object (`parse`/`decode`) and enumerating own-enumerable properties (`stringify`/`encode`) do **not** need new `querystring`-specific ABI symbols — they reuse whatever generic engine primitives already exist for null-prototype object construction (`Object.create(null)` codegen path) and own-enumerable-property enumeration (`Object.keys`/`for...in` codegen path). If no such reusable primitive exists yet at implementation time, that is a blocking dependency to flag against the engine/`rts-primitives`, not something to solve inside `rts-node` (see §5.7 analogue — though this is an *engine* primitive, not an infra-crate one, so it is called out here rather than in 5.7).

### 5.3 Async model

**Fully synchronous — no async model applies.** All six exported functions (`parse`, `stringify`, `escape`, `unescape`, `decode`, `encode`) are plain synchronous calls with no callback parameter, no Promise return, and no event-loop interaction. No tokio runtime, no promise-subsystem, no thread-pool dispatch is needed anywhere in this module. This is one of the few `node:` modules with a zero-async-infra footprint.

### 5.4 Multithread / worker interaction

- The core codec (`escape`/`unescape`) and scan/join primitives are pure functions with no shared state — trivially thread-safe, callable concurrently from any number of RTS threads/regions with no locking.
- The one piece of mutable state is the **overridable `escape`/`unescape` module properties** (§4, §5.1). Node semantics: these are ordinary properties on the `querystring` module-namespace object, which in Node is itself per-`Worker`-instance (each `worker_threads` `Worker` gets its own module registry/instance) — so a reassignment of `querystring.escape` on one thread does **not** affect another thread's `querystring` module object. RTS should map this onto the **`threadLocal`** region of `docs/specs/rts-threading-model.md`: the `escape`/`unescape` override slots live in a per-thread `QuerystringConfig { escape_fn, unescape_fn }`, never promoted to the shared heap, initialized to the native defaults independently on every thread/region — mirroring exactly how `node:dns`'s per-thread `DnsConfig` is specified (see `docs/node-implementation/dns.md` §5.4) and how existing RTS namespaces use `thread_local!`/independent per-thread state (`.claude/rules/02-runtime.md` "State" pattern).
- No `SharedArrayBuffer`/shared-heap concerns — this module exchanges only UTF-8 strings and plain objects, never raw shared memory.
- No `Handle`/`HandleTable` interaction at all (§5.2) — nothing here is a GC-managed rich object beyond the ordinary JS objects/strings the engine already manages generically.

### 5.5 Buffer / TypedArray interop

**None.** `node:querystring` has no byte-buffer surface — every input/output is a UTF-8 `string` (JS string / `StrPtr` at the ABI boundary). No function accepts or returns a `Buffer`, `Uint8Array`, or `ArrayBuffer`, and no base64/binary encoding is involved anywhere in this module (contrast with, e.g., `node:dns`'s `TlsaRecord.data`, which does need such interop — querystring has no analogous field).

### 5.6 Doctrine placement

`node:querystring` is **non-primordial** — the engine (`rts-codegen-new`) must never hardcode `"querystring"` or any of its member names (`parse`, `stringify`, `escape`, `unescape`, `decode`, `encode`). Resolution follows the same data-driven mechanism as every other `node:` module already implemented in `crates/rts-node/src/lib.rs`: `import ... from 'node:querystring'` resolves through `rts_node::ns_prefix_for("node:querystring")` → a fixed `ns_prefix` (e.g. `"node_querystring"`) looked up against `NODE_SPECS` (a plain `&[&NodespaceSpec]` data table, zero hardcoded per-module arm in codegen), and each call such as `querystring.parse(...)` resolves through `rts_node::node_lookup("node_querystring.parse")` to a `NodespaceMember` (`symbol`, `args`, `returns`) — purely data, exactly matching the mechanism already used for `node:dns`/other registered node namespaces.

Native-extern / `.ts`-shim split: the four symbols in §5.2 (`ESCAPE`, `UNESCAPE`, `SPLIT_PAIRS`, `JOIN_PAIRS`) are the entire native surface — raw string-algorithm primitives with no JS-shaped ergonomics baked in. Everything JS-shaped lives in a `.ts` shim (`rts-node/src/querystring/querystring.ts`):
- The public `parse`/`stringify`/`decode`/`encode`/`escape`/`unescape` function exports and their default-parameter handling (`sep = '&'`, `eq = '='`, `options = {}`).
- Constructing the **null-prototype** result object for `parse`/`decode` from the native `SPLIT_PAIRS` JSON array (calling the engine's `Object.create(null)`-equivalent object-construction primitive, then assigning each decoded key — promoting to an array on second-occurrence of a key).
- Enumerating `obj`'s own enumerable properties for `stringify`/`encode`, dispatching per-value type (`string`/`number`/`bigint`/`boolean`/arrays thereof/other→`''`) before calling `JOIN_PAIRS`.
- The **overridable `escape`/`unescape` module properties**: the `.ts` shim exports mutable `let escape = nativeEscapeDefault; let unescape = nativeUnescapeDefault;`-shaped bindings (or an equivalent mutable-property object) so `querystring.escape = customFn` reassignment (§4) works exactly as documented, with `stringify`'s/`parse`'s default codec path reading the *current* value of these bindings at call time.
- Threading the `decodeURIComponent`/`encodeURIComponent` options through to either the fast native `ESCAPE`/`UNESCAPE` calls (default case) or a user-supplied JS callback per key/value (override case), per the design rationale in §5.2.

### 5.7 Shared-infra dependencies (FLAG)

**None.** `node:querystring` needs no promise/async subsystem, no shared tokio runtime, no GC thread-registry hooks, no `HandleTable`/`Handle` slab, no TLS/crypto primitives, and no net/socket primitives. It is pure, synchronous, string-in/string-out (plus one plain-object shape in `parse`/`stringify`) computation entirely containable within `rts-node` and the engine's existing generic object/string primitives (§5.2's "no new ABI needed for null-prototype construction / own-enumerable enumeration" point is the only cross-cutting dependency, and it is on the **engine's already-existing generic object machinery**, not on `rts-std`-owned infra — so it does not belong in this "must be hoisted out of rts-std" list). This makes `node:querystring` one of the simplest, best "first module" candidates for validating the `rts-node` NodespaceSpec/ABI plumbing end-to-end before tackling modules that do need the heavier hoists (`node:dns`, `node:fs/promises`, `node:net`, etc.).

### 5.8 Implementation phases

1. **(a)** Add `rts-node/src/querystring/mod.rs` with the `NodespaceSpec` skeleton (`node_module: "querystring"`, `ns_prefix: "node_querystring"`); register it in `NODE_SPECS`.
2. **(b)** Implement `escape`/`unescape` native codec (`codec.rs`) with unit tests against Node's documented escape/unescape character-set behavior (byte-for-byte comparison against real Node 25 output for the printable ASCII range + a sampling of multi-byte UTF-8 sequences + malformed-percent-escape inputs). Wire `__RTS_FN_NODE_QUERYSTRING_ESCAPE`/`UNESCAPE`.
3. **(c)** Implement the `.ts` shim's `escape`/`unescape` exports as overridable bindings (§5.6) wrapping the phase-(b) native calls by default.
4. **(d)** Implement `SPLIT_PAIRS`/`JOIN_PAIRS` native scan/join primitives (`parse.rs`/`stringify.rs`) operating on already-decoded/to-be-encoded key/value pairs (percent-encoding handled separately per §5.2's design).
5. **(e)** Implement the `.ts` `parse`/`decode` pair: call `SPLIT_PAIRS`, percent-decode each key/value (native `UNESCAPE` fast path, or a user `options.decodeURIComponent` callback), build the null-prototype result object with repeated-key array promotion, honor `maxKeys` (verify truncation happens in the native scan, not the `.ts` post-processing, to avoid decoding work past the cap).
6. **(f)** Implement the `.ts` `stringify`/`encode` pair: enumerate `obj`'s own enumerable properties, coerce/dispatch each value per §4's type rules, percent-encode each key/value (native `ESCAPE` fast path, or a user `options.encodeURIComponent` callback), call `JOIN_PAIRS`.
7. **(g)** Cross-check the null-prototype construction and own-enumerable-property enumeration against whatever generic engine primitives already exist (`Object.create(null)` codegen path, `Object.keys`/`for...in` enumeration path); if either primitive does not yet exist/expose what's needed, flag as a blocking engine-level (not `rts-std`) dependency and coordinate with whoever owns `Object`/shape codegen rather than hand-rolling a querystring-specific object shape.
8. **(h)** Differential test suite against a real Node 25 binary: generate a corpus of query strings (ASCII, UTF-8, empty segments, repeated keys, custom `sep`/`eq`, multi-char `eq`, `maxKeys` edge values, prototype-pollution-shaped keys like `__proto__`/`constructor`/`toString`) and assert RTS's `parse`/`stringify`/`escape`/`unescape` output matches Node's byte-for-byte.

## 6. Test plan

```
tests/node/querystring/querystring_parse_basic.test.ts
  - querystring.parse('foo=bar&abc=xyz&abc=123') => { foo: 'bar', abc: ['xyz', '123'] }
  - querystring.parse('') => {} (empty object, still null-prototype)
  - querystring.parse('foo=bar') => single value is a bare string, not ['bar']
  - querystring.parse('foo') => { foo: '' } (key with no '=' at all)
  - querystring.parse('foo=') => { foo: '' }
  - querystring.parse('=bar') => { '': 'bar' } (empty key)
  - querystring.parse('&=&=') => two empty-string key/value pairs both under key '' (accumulate to ['', ''])
  - querystring.parse('a=1;b=2', ';') => custom separator
  - querystring.parse('a::1**b::2', '**', '::') => custom multi-char sep AND eq simultaneously
  - querystring.parse('a=%E4%B8%AD%E6%96%87') decodes UTF-8 percent-escapes correctly

tests/node/querystring/querystring_parse_prototype.test.ts
  - result of parse(...) has no .hasOwnProperty/.toString/.constructor inherited (Object.getPrototypeOf(result) === null)
  - querystring.parse('__proto__=polluted') => result.__proto__ is an OWN data property equal to 'polluted', and Object.prototype is untouched (({}).polluted === undefined)
  - querystring.parse('constructor=x&toString=y') => both land as own string properties, no crash/shadowing

tests/node/querystring/querystring_parse_maxkeys.test.ts
  - a query string with 2000 distinct keys parsed with default options => result has exactly 1000 keys (first 1000 in order)
  - same input with { maxKeys: 0 } => all 2000 keys present
  - { maxKeys: 5 } => exactly 5 keys retained

tests/node/querystring/querystring_parse_custom_decode.test.ts
  - querystring.parse('w=%D6%D0%CE%C4', null, null, { decodeURIComponent: gbkDecode }) invokes the custom function per value and produces the GBK-decoded string
  - a decodeURIComponent that throws propagates the throw out of parse() (not swallowed)

tests/node/querystring/querystring_stringify_basic.test.ts
  - querystring.stringify({ foo: 'bar', baz: ['qux', 'quux'], corge: '' }) => 'foo=bar&baz=qux&baz=quux&corge='
  - querystring.stringify({ foo: 'bar', baz: 'qux' }, ';', ':') => 'foo:bar;baz:qux'
  - querystring.stringify({}) => ''
  - querystring.stringify({ n: 42, big: 10n, flag: true }) => numeric/bigint/boolean values stringify via String()
  - querystring.stringify({ list: [1, 2, 3] }) => 'list=1&list=2&list=3'
  - querystring.stringify({ bad: {} , u: undefined, nil: null }) => each coerces to '' for its key
  - querystring.stringify({ w: '中文', foo: 'bar' }, null, null, { encodeURIComponent: gbkEncode }) uses the custom encoder

tests/node/querystring/querystring_stringify_enumeration.test.ts
  - only own enumerable properties are serialized: an inherited property via a prototype is NOT included
  - a Symbol-keyed property is not included
  - a non-enumerable property (Object.defineProperty(..., { enumerable: false })) is not included

tests/node/querystring/querystring_escape_unescape.test.ts
  - querystring.escape('a b&c=d') percent-encodes reserved query characters
  - querystring.unescape(querystring.escape(s)) round-trips for a battery of ASCII + UTF-8 strings
  - querystring.unescape('%') (malformed escape) does not throw, returns a best-effort/passthrough result
  - reassigning querystring.escape = customFn changes the encoder used internally by stringify()'s default path
  - reassigning querystring.unescape = customFn changes the decoder used internally by parse()'s default path

tests/node/querystring/querystring_decode_encode_aliases.test.ts
  - querystring.decode(str) === querystring.parse(str) for a battery of inputs (same object shape/values)
  - querystring.encode(obj) === querystring.stringify(obj) for a battery of inputs (identical string output)

tests/node/querystring/querystring_roundtrip.test.ts
  - querystring.parse(querystring.stringify(obj)) reproduces obj for simple flat string/array-of-string shapes
  - round-trip through custom sep/eq preserves shape

tests/node/querystring/querystring_worker_threads.test.ts (multithread)
  - main thread reassigns querystring.escape to a custom function; a spawned worker_thread's querystring.escape is still the native default (per-thread override isolation, §5.4)
  - concurrent stringify()/parse() calls from N worker threads on disjoint inputs all produce correct, non-interfering results (stress test for the "pure function, no shared state" claim in §5.1/§5.4)
```

## 7. Open questions / deferrals

- **Exact `escape()` character set.** Node's `querystring.escape` is documented as "optimized for the requirements of URL query strings" but is not byte-for-byte identical to `encodeURIComponent` in all Node versions/implementations historically. The precise unreserved-character set must be extracted from Node's actual source (`lib/querystring.js` / the internal `QSEscape` binding) at implementation time and diffed against a real Node 25 binary — the WebFetch-sourced docs used for this spec describe *behavior*, not the exact table, so treat any assumption in §5.1 as `(verify)` until cross-checked against source or a running Node 25.
- **Non-finite numeric `stringify()` values (`NaN`/`Infinity`).** §4 flags that Node's documented behavior ("numeric values must be finite") is ambiguous about whether this is merely a recommendation/typing constraint or an enforced runtime behavior difference (e.g. silently coerced vs. producing `"NaN"`/`"Infinity"` in output). Needs empirical verification against real Node 25 before RTS locks in a behavior; do not guess a "safer" behavior that diverges from actual Node output.
- **Null-prototype construction primitive availability.** §5.2/§5.8(g) depends on the engine already exposing (or being extended to expose) a generic `Object.create(null)`-equivalent object-construction primitive and an own-enumerable-property enumeration primitive reusable by `.ts` shims outside the primordial `Object`/`Array` classes themselves. If no such reusable primitive exists at implementation time, this becomes a blocking engine-level dependency (not an `rts-std` hoist, an engine/`rts-primitives` one) that should be raised with whoever owns `Object`/shape codegen before phase (e)/(f) can be completed correctly.
- **`decode`/`encode` as literal aliases vs. separate implementations.** This spec assumes `decode`/`encode` should be implemented as the `.ts` shim binding the same underlying function object as `parse`/`stringify` (true aliasing, so `querystring.decode === querystring.parse` by reference, matching Node's actual implementation where they are the same function value) rather than separate wrapper functions that merely behave identically. Confirm this reference-identity expectation is actually tested/relied upon by any real-world code before deciding whether it's load-bearing enough to require (vs. two independently-implemented functions with matching behavior being an acceptable simplification).
- **Interaction with `node:url`'s `URLSearchParams`.** Out of scope for this module's spec, but worth flagging: some ecosystem code path-switches between `querystring.parse`/`stringify` and `new URLSearchParams(...)` based on a feature-detection or performance heuristic; RTS should ensure both modules are implemented with enough behavioral parity awareness that code doing this comparison doesn't produce surprising divergences beyond what real Node itself exhibits (real Node already has known behavioral differences between the two — no new RTS-only divergence should be introduced).
