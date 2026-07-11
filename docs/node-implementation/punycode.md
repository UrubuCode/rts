# node:punycode

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:punycode` |
| Node.js version | 25.x |
| Stability | 0 - Deprecated (DEP0040: documentation-only deprecation since v7.0.0; `--pending-deprecation` support since v16.6.0; **runtime deprecation** — emits a process warning by default — since v21.0.0; **application deprecation** — Type: "Application (non-`node_modules` code only)", i.e. code inside `node_modules` is exempt from the warning — since v23.7.0/v22.14.0) |
| Tier | P2 (deprecated) |
| Status | [ ] Not implemented — spec only |
| Import forms | `import punycode from "node:punycode"`; `const punycode = require("node:punycode")`; the bare specifier `require("punycode")` (no `node:` prefix) historically resolved to this same built-in but is part of the same deprecation track — RTS should require the `node:` prefix and treat the bare form as **not supported** (verify exact bare-specifier fallback behavior against live Node 25 before matching it; not load-bearing for parity since Node itself is phasing it out) |
| Globals exposed | none (no ambient global; must be explicitly imported) |

## 1. Purpose

`node:punycode` is Node's bundled copy of the userland [Punycode.js](https://github.com/bestiejs/punycode.js) library, implementing the Bootstring/Punycode algorithm from [RFC 3492](https://tools.ietf.org/html/rfc3492) for converting Unicode text to and from an ASCII-only representation, plus a small UCS-2 (UTF-16 code-unit ⇄ Unicode code-point) helper namespace. Its primary historical use is encoding Internationalized Domain Names (IDNs) into the ASCII `xn--` form usable in DNS and URLs. The module is **deprecated** (DEP0040) — Node recommends the userland `punycode` npm package or the WHATWG URL API (`url.domainToASCII`/`domainToUnicode`) instead — but RTS still implements it for drop-in compatibility with existing Node programs and dependencies that `require("node:punycode")` directly.

## 2. Exported API surface (COMPLETE)

### Classes

None. `node:punycode`'s module namespace is a plain object of functions, one nested namespace object (`ucs2`), and one string property (`version`) — there are no classes, no constructors, and no `EventEmitter`-derived types anywhere in this module.

### Top-level functions

| Function | Variant |
|---|---|
| `punycode.decode(string)` | sync |
| `punycode.encode(string)` | sync |
| `punycode.toASCII(domain)` | sync |
| `punycode.toUnicode(domain)` | sync |
| `punycode.ucs2.decode(string)` | sync |
| `punycode.ucs2.encode(codePoints)` | sync |

All six are pure, synchronous, side-effect-free string/array transforms. None has a callback or Promise-returning form in Node — this module predates Node's async conventions entirely and nothing here touches I/O.

#### `punycode.decode(string)`

Converts a Punycode string of ASCII-only characters to a string of Unicode codepoints (the inverse of `encode`). Operates on a **single label** — it does not split on `.` (that splitting is `toUnicode`'s job).

Params:

| Name | Type | Optional | Default |
|---|---|---|---|
| `string` | `string` | no | — |

Return: `string` — the decoded Unicode string.

Throws: `RangeError` ("Illegal input >= 0x80 (not a basic code point)", "Invalid input", "Overflow: input needs wider integers to process") on malformed Punycode input (invalid digit, overflow of the internal bias/delta arithmetic, or a non-ASCII byte where only basic code points are allowed). Variant: sync.

Example: `punycode.decode('maana-pta')` → `'mañana'`; `punycode.decode('--dqo34k')` → `'☃-⌘'`.

#### `punycode.encode(string)`

Converts a string of Unicode codepoints to a Punycode string of ASCII-only characters (the inverse of `decode`). Operates on a single label; does not add the `xn--` prefix (that is `toASCII`'s job).

Params:

| Name | Type | Optional | Default |
|---|---|---|---|
| `string` | `string` | no | — |

Return: `string` — the encoded ASCII-only Punycode string.

Throws: `RangeError` ("Overflow: input needs wider integers to process") only in pathological inputs that overflow the 32-bit-equivalent internal delta arithmetic (practically unreachable for real-world domain labels, but must be a real bounds check, not assumed-unreachable). Variant: sync.

Example: `punycode.encode('mañana')` → `'maana-pta'`; `punycode.encode('☃-⌘')` → `'--dqo34k'`.

#### `punycode.toASCII(domain)`

Converts a Unicode string representing an Internationalized Domain Name (or a full domain with multiple dot-separated labels, optionally prefixed with a `user@` part) to Punycode. Only the **non-ASCII parts** of the domain are converted; each label is processed independently, and ASCII-only labels pass through unchanged (no `xn--` prefix added).

Params:

| Name | Type | Optional | Default |
|---|---|---|---|
| `domain` | `string` | no | — |

Return: `string` — the Punycode-encoded domain (each non-ASCII label becomes `xn--` + `encode(label)`; ASCII labels are untouched).

Throws: `RangeError` propagated from an underlying `encode()` call on a pathological label (see above). Variant: sync.

Example: `punycode.toASCII('mañana.com')` → `'xn--maana-pta.com'`; `punycode.toASCII('☃-⌘.com')` → `'xn----dqo34k.com'`; `punycode.toASCII('example.com')` → `'example.com'` (no-op, all-ASCII).

#### `punycode.toUnicode(domain)`

Converts a domain name containing one or more Punycode-encoded (`xn--`-prefixed) labels back to Unicode. Only labels that start with the case-insensitive `xn--` prefix are decoded; every other label passes through unchanged.

Params:

| Name | Type | Optional | Default |
|---|---|---|---|
| `domain` | `string` | no | — |

Return: `string` — the Unicode-decoded domain.

Throws: `RangeError` propagated from an underlying `decode()` call on a malformed `xn--` label. Variant: sync.

Example: `punycode.toUnicode('xn--maana-pta.com')` → `'mañana.com'`; `punycode.toUnicode('xn----dqo34k.com')` → `'☃-⌘.com'`; `punycode.toUnicode('example.com')` → `'example.com'` (no-op, no `xn--` labels).

#### `punycode.ucs2.decode(string)`

Reads a JS string (UTF-16 code units, with surrogate pairs combined) and returns an array of the numeric Unicode codepoint values it represents. Correctly combines high/low surrogate pairs into a single codepoint above `0xFFFF` instead of returning the two surrogate halves separately (unlike naively iterating `string.charCodeAt`).

Params:

| Name | Type | Optional | Default |
|---|---|---|---|
| `string` | `string` | no | — |

Return: `number[]` (documented as `<integer[]>`) — one entry per Unicode codepoint, **not** per UTF-16 code unit.

Throws: none (malformed/lone surrogates are passed through as their own code-unit value; this function never throws in Node's implementation).

Example: `punycode.ucs2.decode('abc')` → `[0x61, 0x62, 0x63]`; `punycode.ucs2.decode('𝌆')` → `[0x1D306]` (one surrogate pair → one codepoint).

#### `punycode.ucs2.encode(codePoints)`

The inverse of `ucs2.decode`: creates a string from an array of numeric codepoint values, encoding any codepoint above `0xFFFF` as a UTF-16 surrogate pair.

Params:

| Name | Type | Optional | Default |
|---|---|---|---|
| `codePoints` | `number[]` (`<integer[]>`) | no | — |

Return: `string`.

Throws: `RangeError` (`"Invalid code point: <n>"`, standard JS behavior via `String.fromCodePoint`-equivalent logic) if an entry is not a valid Unicode codepoint (negative, non-integer, or > `0x10FFFF`).

Example: `punycode.ucs2.encode([0x61, 0x62, 0x63])` → `'abc'`; `punycode.ucs2.encode([0x1D306])` → `'𝌆'`.

### Properties & constants

| Name | Type | Notes |
|---|---|---|
| `punycode.version` | `string` | Identifies the bundled Punycode.js version (e.g. `'2.3.1'` — **(verify)** exact string against the live Node 25 source; the value tracks whichever upstream Punycode.js release Node vendored, not Node's own version). Read-only in practice (nothing in the documented surface mutates it, though the underlying object property is not literally frozen in Node's implementation). |
| `punycode.ucs2` | `{ decode, encode }` | Namespace object grouping the two UCS-2 helpers (documented separately above under Top-level functions for exhaustiveness, since each is independently callable: `punycode.ucs2.decode`, `punycode.ucs2.encode`). |

No other properties, no `Symbol.for(...)` well-known re-exports, no numeric/error-code constants (unlike e.g. `os.constants` or `fs.constants` — this module defines none).

### Events

None. `node:punycode` exposes no `EventEmitter`, no event-emitting object, and no lifecycle events of any kind — every export is a pure function or a plain-data property.

## 3. Types & option objects

Node's punycode module takes no option objects — every parameter is a bare `string` or `number[]`. For completeness, the shape of the module's own namespace object:

```ts
/** The full shape of `node:punycode`'s module export. */
interface PunycodeModule {
  decode(string: string): string;
  encode(string: string): string;
  toASCII(domain: string): string;
  toUnicode(domain: string): string;
  ucs2: PunycodeUcs2;
  readonly version: string;
}

/** `punycode.ucs2` namespace — UTF-16 code-unit <-> Unicode code-point conversion. */
interface PunycodeUcs2 {
  /** Decodes a JS string into an array of Unicode codepoints (surrogate pairs combined). */
  decode(string: string): number[];
  /** Encodes an array of Unicode codepoints into a JS string (surrogate pairs split). */
  encode(codePoints: number[]): string;
}
```

No callback signatures, no Promise-returning overloads, no error-shape objects beyond the standard `RangeError` thrown directly by `decode`/`encode`/`ucs2.encode` on malformed input (there is no Node-specific `NodeJS.ErrnoException`/`err.code` here — these are plain JS `RangeError`s with a human-readable `message`, not `errno`-style codes).

## 4. Node semantics & edge cases

- **Domain splitting rules (`toASCII`/`toUnicode` only).** The domain string is first split on an `@` if present (an email-like `user@domain` or URL-userinfo form): only the part **after** the last `@` is processed as the domain; the part before (plus the `@`) is passed through unchanged and re-prepended. The domain part is then split into labels on `.` **and** on the fullwidth/ideographic alternate separators U+3002 (`。`), U+FF0E (`．`), and U+FF61 (`｡`) — all four are normalized to ASCII `.` before splitting, matching WHATWG domain-to-ASCII label-separator handling. Each label is processed independently and rejoined with ASCII `.`.
- **`toASCII` per-label rule:** if a label contains any character outside `\x00`-`\x7F` (tested via a non-ASCII regex, not a full IDNA `Bidi`/`ContextJ` validation — this module does **not** implement full IDNA2008 validation, only the Bootstring transcoding), the label becomes `"xn--" + encode(label)`. ASCII-only labels are left byte-for-byte unchanged (no case-folding, no punycode processing at all).
- **`toUnicode` per-label rule:** if a label starts with the case-insensitive prefix `xn--` (matched via a case-insensitive regex, not literal-lowercase-only), the label becomes `decode(label.slice(4))` (the label's trailing part after stripping the 4-char prefix is passed to `decode` as-is — Node's implementation lowercases the whole label first via `toLowerCase()` before the prefix check and decode call). Labels without that prefix are left unchanged.
- **No full IDNA/UTS-46 processing.** This module implements ONLY the RFC 3492 Bootstring algorithm plus the naive `xn--`-prefix/domain-splitting wrapper described above — it does **not** perform Unicode normalization (NFC), case-folding beyond the simple `toLowerCase()` noted above, disallowed-character mapping, or Bidi/ContextJ rule checks that a real IDNA/UTS-46 implementation (and the WHATWG URL API's `domainToASCII`) performs. A string that passes through `punycode.toASCII` unchanged may still be an invalid or unsafe domain per the full IDNA spec — Node's own docs point users at `url.domainToASCII`/the WHATWG URL API for that stricter behavior.
- **Encoding is on Unicode codepoints, not UTF-16 code units.** `encode`/`decode` operate on the codepoint sequence (astral characters above `0xFFFF`, e.g. emoji, count as one code unit for Bootstring purposes), which is exactly why `ucs2.decode`/`ucs2.encode` exist — internally `encode`/`toASCII` first run the input through UCS-2 decode to get true codepoints before running Bootstring, and `decode`/`toUnicode` run UCS-2 encode on the resulting codepoints to produce the final JS string. A caller who naively iterates `string.charCodeAt(i)` on an astral-containing string would see the surrogate halves as two separate values and get wrong Bootstring output — RTS's implementation must decode to real codepoints first, exactly like Node's.
- **No platform (Windows vs POSIX) differences.** This is pure in-memory string/array computation with no filesystem, network, or OS-locale interaction of any kind — behavior is byte-identical across all platforms.
- **No errno/error-code taxonomy.** Errors thrown are plain `RangeError`s (see §3); there is no `err.code` (`ERR_*`) the way most other Node core modules attach one. RTS should throw the equivalent plain `RangeError` with a matching (or at least equivalent-intent) message, not synthesize an `ERR_*` code that Node itself never assigns here.
- **Ordering / backpressure / async guarantees:** not applicable — every function is synchronous, single-threaded-computation, non-blocking (no I/O), and returns its full result in one call; there is no streaming, chunking, or partial-result concept anywhere in this module.
- **Deprecation status is module-wide, not per-function.** All six functions (plus `version`) are deprecated identically as of DEP0040 — there is no finer-grained per-method deprecation. The deprecation is about *this bundled copy* being removed someday, not about the Bootstring algorithm itself being wrong; user code migrating away should switch to the userland `punycode` npm package (same algorithm, actively maintained) or, for IDN/URL purposes specifically, `url.domainToASCII`/`domainToUnicode` (which additionally do full IDNA/UTS-46 processing this module does not).
- **Security note:** because this module does *not* do full IDNA validation (no Bidi/ContextJ/disallowed-character checks, no NFC normalization), using `toASCII`/`toUnicode` directly for security-sensitive domain comparison/display (e.g. anti-homograph-phishing checks) is unsound — Node's own docs implicitly steer this use case toward the WHATWG URL API. RTS's spec/tests should not present this module as IDN-safe; it is a straight RFC 3492 transcoder only.

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node` owns this module fully as a **pure computational** unit — no filesystem, network, process, or OS dependency of any kind, and therefore no dependency surface on `rts-std` to flag beyond the shared GC/string-allocation primitives every `rts-node` module already needs (see §5.7). The core RFC 3492 Bootstring algorithm (`decode`/`encode`) is implemented directly in Rust inside `rts-node::punycode` — it is a small, well-specified, self-contained algorithm (roughly 100-150 lines for encode+decode combined, matching the size of the reference/userland implementations); no external crate is required, though a vetted crate (e.g. `idna`'s internal punycode module, if already a transitive dependency of nothing else in `rts-node`) could be used instead of hand-rolling if it simplifies maintenance — either way, the algorithm parameters (`base=36`, `tmin=1`, `tmax=26`, `skew=38`, `damp=700`, `initial_bias=72`, `initial_n=128`, delimiter `-` (0x2D)) are fixed by RFC 3492 and must match exactly for cross-implementation interop with real-world `xn--` domains.

Per the "no high-level API in Rust" design rule, the domain-splitting/label-mapping logic (`toASCII`/`toUnicode`'s `@`-split, alternate-separator normalization, per-label `xn--`-prefix test, and rejoin) is **not** native — it is plain `.ts` logic in `rts-node`'s TS shim, calling the two native `encode`/`decode` primitives per label. Similarly, `ucs2.decode`/`ucs2.encode` need **no native extern at all**: they are pure UTF-16 ⇄ codepoint conversions directly expressible over the engine's own primordial `String`/`Array` representation (surrogate-pair-aware string iteration and `String.fromCodePoint`-equivalent construction are already engine/primitive-owned operations — punycode's `.ts` shim just calls the primordial JS operations already available to any TS program, it does not need a bespoke native round-trip for this piece).

### 5.2 ABI surface

Only two native externs are needed — the Bootstring core. Everything else in the public surface (`toASCII`, `toUnicode`, `ucs2.decode`, `ucs2.encode`, `version`) is a `.ts` shim built on top of these two plus primordial String/Array operations.

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_PUNYCODE_ENCODE` | `StrPtr(input)` | `StrPtr` | runs RFC 3492 Bootstring encode on the UTF-8 input (internally decoded to Unicode codepoints, never raw UTF-8 bytes); on internal overflow, sets the thread-local error slot (see `guards.rs`/coercion-authority convention) so the `.ts` layer can surface a `RangeError` |
| `__RTS_FN_NODE_PUNYCODE_DECODE` | `StrPtr(input)` | `StrPtr` | runs RFC 3492 Bootstring decode; sets the thread-local error slot on malformed input (invalid digit / overflow) for the `.ts` layer to raise `RangeError` |

Both return a GC-allocated UTF-8 buffer via the standard `StrPtr` (2-slot `ptr`+`len`) convention — no opaque `Handle` is needed anywhere in this module: there is no rich/stateful object (no class, no resource, no cross-call state) to hand out a handle for. `version` is a compile-time `.ts` string constant (no extern needed — it identifies the *bundled algorithm implementation's* version, which is fixed at build time, not runtime-computed).

Native-extern vs `.ts`-shim split:
- **Native externs**: `ENCODE`/`DECODE` (the Bootstring core) only.
- **`.ts` shim** (`rts-node`'s TS surface for this module): `toASCII`/`toUnicode` (domain `@`-split, alternate-separator normalization to `.`, per-label `xn--` test/prefix-add via calls to the two externs above), `ucs2.decode`/`ucs2.encode` (pure primordial String/Array codepoint iteration, no extern call), and the `version` constant.

### 5.3 Async model

Entirely synchronous — Node's punycode API has no callback or Promise-returning form for any of its six functions, and RTS must not add one. No interaction with the RTS event loop, the Promise subsystem, or the shared tokio runtime anywhere in this module: every call is a pure, immediately-returning, single-threaded string/array transform (`variant: sync` for all six, per §2).

### 5.4 Multithread / worker interaction

Fully stateless and thread-safe by construction — `encode`/`decode` read only their input argument and write only their return value, with no shared mutable state, no per-instance handle, and no global/module-level state of any kind (unlike, say, `node:console`'s per-instance timers/counters). Under `docs/specs/rts-threading-model.md` this module needs **no** special per-thread-region or shared-heap treatment: it is safely callable concurrently from any number of RTS threads/worker regions with zero locking, because there is nothing to lock. `worker_threads.Worker` interaction is a non-issue — a worker calling `punycode.toASCII(...)` behaves identically to the main thread calling it, with no cross-thread handle, channel, or `SharedArrayBuffer` involvement whatsoever.

### 5.5 Buffer / TypedArray interop

None. Every documented function operates on `string`/`number[]`, never on `Buffer`/`Uint8Array`/`ArrayBuffer`. `ucs2.decode`/`ucs2.encode` deal with **Unicode codepoints as plain JS numbers in a regular `Array`** (Node's own docs type them `<integer[]>`, a plain array, not a typed array) — RTS must match this exactly and return/accept a primordial `Array` of numbers, **not** a `Uint32Array` or other TypedArray, even though a TypedArray would be a tempting perf optimization; changing the returned container type would be an observable, Node-incompatible behavior change (e.g. `Array.isArray(punycode.ucs2.decode(x))` must stay `true`). No ABI-level byte-buffer crossing is needed anywhere in this module — `StrPtr` (UTF-8 text) is the only cross-boundary payload shape used.

### 5.6 Doctrine placement

Confirmed **non-primordial**: `punycode` has no native literal/syntactic form (no `punycode`-flavored string literal, no special syntax) — it is a plain userland-shaped utility library reached via `import`/`require`, exactly the "no native syntax ⇒ indirect" case in the Primordial doctrine. The engine must never hardcode the name `"punycode"` anywhere in `crates/rts-codegen-new/`.

`import ... from "node:punycode"` resolves through the standard rts-node data table exactly like every other `node:X` import: `ns_prefix_for("node:punycode")` → `"node_punycode"` via the `NODE_SPECS` data table (`NodespaceSpec { node_module: "punycode", ns_prefix: "node_punycode", members: [encode, decode] }`), then `node_lookup("node_punycode.encode"/"node_punycode.decode")` resolves each qualified call to a `NodespaceMember { symbol: "__RTS_FN_NODE_PUNYCODE_ENCODE"/"__RTS_FN_NODE_PUNYCODE_DECODE", .. }` — a pure data lookup, zero `match "punycode" => ...` arms in codegen, identical mechanism to `node:console`/`node:fs`/`node:path`. The higher-level `toASCII`/`toUnicode`/`ucs2.*`/`version` surface lives entirely in the `.ts` shim layer described in §5.1/§5.2 and never touches codegen at all — it is ordinary TS calling the two registered native functions plus primordial String/Array operations.

### 5.7 Shared-infra dependencies (FLAG)

**None.** This module needs no async/event-loop infra, no tokio runtime, no promise-settle path, no TLS/rustls, no crypto primitives, no net sockets — it is pure, synchronous, in-memory string computation. The only substrate dependency is the same one every `rts-node` module has unconditionally: GC-backed string allocation for `StrPtr` returns (a handle-free, engine-owned primitive already available via `rts-engine`, not `rts-std` — see the crate-partition doctrine; `rts-engine` is the acyclic base every crate, including `rts-primitives` and now `rts-node`, is expected to sit on top of). Nothing here needs hoisting out of `rts-std` because nothing here depends on `rts-std` in the first place.

### 5.8 Implementation phases

1. **(a) Bootstring core in Rust** (`rts-node::punycode::bootstring`): implement `encode(codepoints: &[u32]) -> Result<String, PunycodeError>` and `decode(input: &str) -> Result<Vec<u32>, PunycodeError>` per RFC 3492 with the fixed parameter set (`base=36, tmin=1, tmax=26, skew=38, damp=700, initial_bias=72, initial_n=128, delimiter='-'`), unit-testable in pure Rust with no engine/ABI involvement (reuse the four `punycode.decode`/`encode` examples from §2 as the first unit-test vectors, since they are Node's own documented examples).
2. **(b) String ⇄ codepoint bridging**: wrap the core so `ENCODE`/`DECODE` externs accept/return UTF-8 `StrPtr` — `ENCODE` first walks the input UTF-8 string into Unicode scalar values (Rust `char`s map directly to Unicode codepoints, so this is a straightforward `str::chars()` pass, no manual UTF-16 surrogate handling needed on the Rust side since Rust strings are UTF-8/codepoint-based already), calls the core, then formats the ASCII-only Punycode result back into a `StrPtr`. `DECODE` calls the core to get codepoints, then builds a UTF-8 `String` from them (`char::from_u32` per codepoint, erroring via the thread-local error slot if the algorithm ever produces an out-of-range value — should not happen for well-formed input, but must not panic/UB on malformed input).
3. **(c) Wire `ENCODE`/`DECODE` into `NODE_SPECS`** as `node_punycode.encode`/`node_punycode.decode`, register the `__RTS_FN_NODE_PUNYCODE_ENCODE`/`_DECODE` symbols, confirm `rts ir` shows a direct extern call with no unexpected boxing for the `StrPtr` round-trip.
4. **(d) `.ts` shim — `decode`/`encode`**: thinnest possible wrapper calling the two externs directly; this alone should make `punycode.decode`/`punycode.encode` fully Node-compatible for single labels.
5. **(e) `.ts` shim — `ucs2.decode`/`ucs2.encode`**: pure TS using the engine's primordial string iteration (codepoint-aware `for...of` over a string) and `String.fromCodePoint`; no native call.
6. **(f) `.ts` shim — `toASCII`/`toUnicode`**: implement the `@`-split, alternate-separator (`。`/`．`/`｡`) normalization to `.`, per-label split/map/rejoin, and the case-insensitive `xn--` prefix test, calling `encode`/`decode` per non-ASCII/`xn--` label respectively (see §4 for the exact per-label rules).
7. **(g) `version` constant**: hardcode the bundled-algorithm version string in the `.ts` shim (confirm the exact value to match against Node 25's actual bundled Punycode.js version — flagged "(verify)" in §2/§7).
8. **(h) Module wiring**: register `"punycode"` in the `node_module`/`ns_prefix` table alongside the other `NODE_SPECS` entries, confirm both `import punycode from "node:punycode"` and named imports (`import { toASCII } from "node:punycode"`) resolve.
9. **(i) Deprecation warning**: decide whether RTS's `require`/`import` machinery emits an equivalent process warning on first use of `node:punycode` (mirroring DEP0040's runtime-deprecation behavior) — track as a policy question in §7 rather than blocking the functional implementation on it.

## 6. Test plan

`tests/node_punycode_basic.test.ts`:
- `punycode.decode('maana-pta')` → `'mañana'`; `punycode.decode('--dqo34k')` → `'☃-⌘'` (Node's own documented examples, exact expected strings).
- `punycode.encode('mañana')` → `'maana-pta'`; `punycode.encode('☃-⌘')` → `'--dqo34k'`.
- Round-trip property: for a range of sample strings (ASCII-only, single accented char, mixed ASCII+non-ASCII, full-astral emoji), `decode(encode(s)) === s`.

`tests/node_punycode_domain.test.ts`:
- `punycode.toASCII('mañana.com')` → `'xn--maana-pta.com'`.
- `punycode.toASCII('☃-⌘.com')` → `'xn----dqo34k.com'`.
- `punycode.toASCII('example.com')` → `'example.com'` (all-ASCII no-op, confirms non-ASCII parts only are converted).
- `punycode.toUnicode('xn--maana-pta.com')` → `'mañana.com'`.
- `punycode.toUnicode('xn----dqo34k.com')` → `'☃-⌘.com'`.
- `punycode.toUnicode('example.com')` → `'example.com'` (no-op, no `xn--` label).
- Multi-label mixed domain: one ASCII label + one non-ASCII label + one already-`xn--` label in the same string, confirm each is handled independently and correctly rejoined with `.`.
- `xn--` prefix matched case-insensitively (`'XN--maana-pta.com'` still decodes) per §4.
- Alternate separator characters (U+3002/U+FF0E/U+FF61) in place of `.` are normalized before splitting.
- `user@domain` form: only the part after `@` is transcoded, the `user@` prefix passes through unchanged.

`tests/node_punycode_ucs2.test.ts`:
- `punycode.ucs2.decode('abc')` → `[0x61, 0x62, 0x63]`.
- `punycode.ucs2.decode('𝌆')` → `[0x1D306]` (surrogate pair combined into one codepoint, not two).
- `punycode.ucs2.encode([0x61, 0x62, 0x63])` → `'abc'`.
- `punycode.ucs2.encode([0x1D306])` → `'𝌆'` (astral codepoint split into a surrogate pair).
- Round-trip: `ucs2.encode(ucs2.decode(s)) === s` for a string mixing BMP and astral characters.
- `Array.isArray(punycode.ucs2.decode('abc'))` is `true` (confirms plain `Array`, not a TypedArray — see §5.5).

`tests/node_punycode_errors.test.ts`:
- `punycode.decode('invalid input with spaces')` (or another malformed Punycode string containing a disallowed character/invalid digit) throws `RangeError`.
- `punycode.ucs2.encode([-1])` and `punycode.ucs2.encode([0x110000])` (out-of-range codepoint) both throw `RangeError`.
- Confirm no crash/hang/`ACCESS_VIOLATION` on any malformed input — every failure path must be a clean thrown `RangeError`, never a native panic.

`tests/node_punycode_module_shape.test.ts`:
- `require('node:punycode')` and `import punycode from 'node:punycode'` both expose `decode`/`encode`/`toASCII`/`toUnicode`/`ucs2`/`version`.
- `typeof punycode.version === 'string'`.
- Named import form `import { toASCII, toUnicode } from 'node:punycode'` resolves the same functions as the default-export properties.

Multithread: not applicable as a dedicated test file — §5.4 establishes this module is trivially thread-safe (pure functions, no shared state), so a spot-check of `punycode.toASCII(...)` called concurrently from two RTS worker threads with the same/different inputs (asserting no corruption/no shared-state bleed) is a reasonable addition to `tests/node_punycode_basic.test.ts` rather than a standalone multithread fixture, given there is no per-instance or per-thread state to isolate in the first place.

## 7. Open questions / deferrals

- **Exact `punycode.version` string** to match against live Node 25 — WebFetch on the live `punycode.html` doc page did not return the literal bundled version number; must be confirmed against Node 25's actual source (`lib/punycode.js` or wherever it now lives internally) before implementing. Marked "(verify)" in §2 and §5.8 step (g).
- **Bare specifier `require('punycode')` (no `node:` prefix) behavior in Node 25** — historically Node allowed requiring core modules without the `node:` prefix, but punycode's deprecation track (DEP0040, now at "application deprecation" as of v23.7.0/v22.14.0) may have changed whether the bare specifier still resolves to the built-in vs. falls through to a `node_modules`-installed userland `punycode` package (which is exactly the migration Node is nudging users toward). Recommend RTS require the `node:` prefix explicitly and treat the bare form as out of scope for parity, rather than reverse-engineering Node's exact bare-specifier resolution edge case for a module Node itself is deprecating.
- **Whether RTS emits an equivalent deprecation warning** on `import`/`require` of `node:punycode`, mirroring DEP0040's "runtime deprecation" (process warning by default since v21.0.0) and "application deprecation" (non-`node_modules` code only, since v23.7.0/v22.14.0) semantics. This is a policy question shared with every other deprecated Node module RTS implements (there is currently no general "deprecation warning" facility described elsewhere in the RTS docs) — deferred to whichever module/PR first establishes that general mechanism, rather than inventing a one-off for `punycode` alone.
- **Whether to vendor a small crate (e.g. `idna`'s internal punycode submodule) vs. hand-roll the ~100-150 line Bootstring core** in `rts-node` — left as an implementation-time choice in §5.8 step (a); either is compliant with the "fully independent crate, no rts-std dependency" decision as long as the chosen dependency (if any) is scoped to `rts-node` only and does not reintroduce a `rts-std` coupling.
- **Full IDNA/UTS-46 parity is explicitly out of scope** for this module (see §4's security note) — if/when RTS implements the WHATWG URL API's `domainToASCII`/`domainToUnicode` (a separate, stricter spec surface), that implementation should **not** simply delegate to `node:punycode`'s `toASCII`/`toUnicode`, since Node's own real implementation does not either; track as a note for whoever specs `node:url`'s WHATWG surface, not a blocker here.
