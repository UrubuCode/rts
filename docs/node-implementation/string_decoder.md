# node:string_decoder

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:string_decoder` |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P1 |
| Status | [x] Implemented — `StringDecoder` (constructor default/encoding + `ERR_UNKNOWN_ENCODING`, `write`, `end`+buffer, `text(buffer, offset)` legacy, `encoding` getter) for utf8/utf16le/base64/base64url/latin1/ascii/hex with full multi-byte/-unit boundary handling + U+FFFD flush + reuse-after-end; accepts string / Uint8Array / Buffer input. Object-backed Registry class. Tests: tests/node_string_decoder.test.ts 11/11. 
| Import forms | `import { StringDecoder } from "node:string_decoder"`; CJS `require("node:string_decoder")` / legacy bare `require("string_decoder")` |
| Globals exposed | None — `node:string_decoder` does not add anything to `globalThis` |

## 1. Purpose

`node:string_decoder` decodes a stream of raw bytes (delivered in arbitrarily
chopped chunks, e.g. from a socket or file read) into correct JavaScript
strings, while preserving multi-byte characters that straddle a chunk
boundary. It exists specifically because the naive
`Buffer.prototype.toString(encoding)` applied independently to each chunk
would corrupt a multi-byte character (UTF-8 up to 4 bytes, UTF-16LE surrogate
pairs, base64/base64url 3-byte groups) whenever the split falls mid-character.
The module exposes exactly one class, `StringDecoder`, with a two-method
public surface (`write`/`end`); it holds no top-level functions, no
constants, and is not an `EventEmitter`.

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `class StringDecoder`

Not an `EventEmitter`, not a subclass of anything — a plain class with
internal-only state (encoding + a small pending-bytes buffer for the
in-progress multi-byte/multi-unit character, if any).

**Constructor**

```typescript
new StringDecoder(encoding?: string)
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `encoding` | `string` | yes | `'utf8'` |

Throws: `TypeError` (`ERR_UNKNOWN_ENCODING`) if `encoding` is truthy and not
one of the encodings `Buffer.isEncoding()` recognizes (see §4 for the
normalization table). Added v0.1.99.

**Instance methods** (2 documented + 1 legacy/undocumented):

| Method | Signature |
|---|---|
| `write` | `write(buffer: string \| Buffer \| TypedArray \| DataView): string` |
| `end` | `end(buffer?: string \| Buffer \| TypedArray \| DataView): string` |
| `text` *(undocumented/legacy)* | `text(buffer: Buffer, offset: number): string` |

**`write(buffer)`** — Added v0.1.99; history note: since v8.0.0 each
invalid/incomplete byte sequence collapses to a **single** U+FFFD
replacement character (previously one U+FFFD per orphaned byte).

| Param | Type | Optional | Default |
|---|---|---|---|
| `buffer` | `string \| Buffer \| TypedArray \| DataView` | no | — |

Returns: `string` — the decoded portion of `buffer`, holding back any
trailing bytes that are (so far) an incomplete multi-byte/multi-unit
character in an internal buffer for the *next* `write()`/`end()` call.
Throws: nothing documented (malformed input degrades to U+FFFD, it never
throws). Variant: **sync**. If `buffer` is a JS `string`, Node first runs it
through `Buffer.from(buffer)` (implicit UTF-8 re-encode) before decoding —
see §4.

**`end(buffer?)`** — Added v0.9.3.

| Param | Type | Optional | Default |
|---|---|---|---|
| `buffer` | `string \| Buffer \| TypedArray \| DataView` | yes | — |

Returns: `string` — if `buffer` is given, one final internal `write(buffer)`
is performed first; then any bytes still held in the internal pending buffer
are flushed, with incomplete UTF-8/UTF-16LE sequences replaced by the
appropriate substitution character(s) (U+FFFD). After `end()` returns, the
same `StringDecoder` instance is fully reusable for a fresh sequence of
`write()` calls (all pending-state counters reset to zero). Throws: nothing
documented. Variant: **sync**.

**`text(buffer, offset)`** *(undocumented/legacy, present on the prototype)* —
the per-encoding "detect a trailing incomplete sequence starting at `offset`
and return the decodable prefix" dispatcher that `write()` calls internally
(`utf8Text`/`utf16Text`/`base64Text` per encoding; the "simple" encodings —
ascii/latin1/hex — use a trivial `buf.toString(encoding, offset)` with no
incomplete-sequence concept at all). Some older ecosystem packages (older
`readable-stream` versions) poked at this directly. Not part of the official
documented API; RTS should still provide it on the `.ts` shim's prototype for
maximal drop-in compatibility, but it is not a target for the native ABI
surface (§5.8 phase d).

**Instance properties** (none documented; 3 legacy/undocumented, present on
the real Node object for backwards compatibility with code that inspected
internal decoder state):

| Property | Type | Notes |
|---|---|---|
| `encoding` *(effectively documented via constructor)* | `string` | The normalized encoding name (e.g. `'utf16le'` for both `'ucs2'` and `'utf16le'` input). |
| `lastChar` *(legacy/undocumented)* | `Buffer` | Fixed-size scratch buffer (4 bytes for utf8/utf16le, 3 for base64/base64url) holding the pending incomplete-sequence bytes. |
| `lastNeed` *(legacy/undocumented)* | `number` | How many more bytes are needed to complete the pending sequence. |
| `lastTotal` *(legacy/undocumented)* | `number` | Total byte length of the pending sequence once complete. |

These three are implementation-detail getters/fields, not guaranteed API —
see §5.8 phase d / §7 for RTS's compat stance.

### 2.2 Top-level functions

None. The module's only export is the `StringDecoder` class itself
(`functionCount` = 0 for this spec's bookkeeping).

### 2.3 Properties & constants

None. `node:string_decoder` exports no module-level constants.

### 2.4 Events

None. `StringDecoder` is not an `EventEmitter` and emits nothing.

## 3. Types & option objects

```typescript
type BufferLike = string | Buffer | Uint8Array | ArrayBufferView; // TypedArray | DataView

// The encoding names StringDecoder (and Buffer.isEncoding) accept.
// See §4 for the full normalization/alias table.
type StringDecoderEncoding =
  | "utf8" | "utf-8"
  | "utf16le" | "utf-16le" | "ucs2" | "ucs-2"
  | "latin1" | "binary"
  | "base64" | "base64url"
  | "ascii"
  | "hex";

// Informative only — not part of the public API, describes the
// internal per-instance state RTS's native side must track.
interface DecoderPendingState {
  encoding: "utf8" | "utf16le" | "base64" | "simple"; // dispatch family
  lastNeed: number;   // 0 when nothing pending
  lastTotal: number;  // 0, 2, 3, or 4
  lastChar: Uint8Array; // fixed-size scratch (len 3 or 4 depending on family)
}
```

## 4. Node semantics & edge cases

### Encoding normalization (alias table)

| Input (case-insensitive) | Normalizes to |
|---|---|
| `utf8`, `utf-8`, `''`/`null`/`undefined` (default) | `utf8` |
| `ucs2`, `ucs-2`, `utf16le`, `utf-16le` | `utf16le` |
| `latin1`, `binary` | `latin1` |
| `base64` | `base64` |
| `base64url` | `base64url` |
| `ascii` | `ascii` |
| `hex` | `hex` |
| anything else | throws `TypeError [ERR_UNKNOWN_ENCODING]: Unknown encoding: <value>` at construction time |

### Dispatch families (how buffering behaves per encoding)

Node internally groups the 7 normalized encodings into **4 behavioral
families**, each with distinct internal-state handling. RTS's native
implementation must reproduce these families' *observable behavior*
byte-for-byte; it does not need to reproduce Node's exact internal function
names.

**1. `utf8`** — variable 1–4 byte sequences.

- On `write(buf)`: any bytes at the end of `buf` that begin a valid UTF-8
  lead byte but don't yet have all of their continuation bytes are held back
  (not included in the returned string) and prepended to the *next*
  `write()`'s input.
- Continuation-byte validation happens eagerly: as soon as a continuation
  byte is read that does **not** match `0b10xxxxxx`, the pending sequence is
  abandoned and immediately replaced by exactly **one** U+FFFD — Node does
  not wait indefinitely hoping the sequence completes.
- `end()`: any sequence still pending (never completed) is replaced by
  exactly one U+FFFD, regardless of how many bytes of it were held (1, 2, or
  3 — always exactly one replacement character, per the v8.0.0 semantics
  change noted in §2.1).
- Rust mapping insight (§5.1): `std::str::from_utf8`'s `Utf8Error` already
  distinguishes "incomplete sequence at the end of input" (`error_len() ==
  None`) from "genuinely invalid sequence" (`error_len() == Some(n)`), and
  `valid_up_to()` gives exactly the split point Node's algorithm computes by
  hand — this is a strong, idiomatic native mapping, not a reimplementation
  of Node's byte-shifting logic from scratch.

**2. `utf16le`** (also reached via the `ucs2` alias) — 2-byte code units,
surrogate pairs are 2 code units (4 bytes).

- If the input ends on an **odd** byte count, the trailing single byte is
  held back (`lastNeed = 1`, `lastTotal = 2`).
- If the input ends on an **even** byte count but the *last decoded UTF-16
  code unit* is a high surrogate (`0xD800`–`0xDBFF`), those trailing 2 bytes
  are held back too (`lastNeed = 2`, `lastTotal = 4`) so they can be
  recombined with a low surrogate that may arrive in the next `write()` —
  this produces the correct astral (surrogate-pair) character when the pair
  is split exactly at the code-unit boundary between chunks.
- `end()`: a still-pending lone byte, or a still-pending unpaired high
  surrogate, is replaced by exactly one U+FFFD.
- Unlike utf8, there is **no continuation-byte validity check** for
  utf16le — every 2-byte group is inherently "valid" as a UTF-16 code unit
  (including lone/unpaired surrogates, which JS strings tolerate).

**3. `base64` / `base64url`** — encode in fixed groups of **3 input bytes →
4 output characters**.

- `write(buf)`: bytes are consumed in the largest possible multiple of 3;
  the remainder (`buf.length % 3`, i.e. 0, 1, or 2 leftover bytes) is held
  back rather than encoded, so that no `write()` call ever emits a partial
  4-character group that would need un-padding/re-padding across the
  boundary.
- `end()`: the held-back 1 or 2 bytes (if any) are finally encoded as a
  final short group. For `base64`, standard `=`/`==` padding is applied
  (bytes-remainder 1 → `==`, 2 → `=`); for `base64url`, there is **no**
  padding at all (by definition of the URL-safe variant) and the group is
  simply shorter.
- The 3-byte grouping logic is identical between `base64` and `base64url`;
  they differ only in output alphabet (`+/` vs `-_`) and padding presence.

**4. "Simple" encodings — `ascii`, `latin1`/`binary`, `hex`** — no
buffering at all.

- Every `write(buf)` call independently and fully decodes `buf` in one shot
  (`buf.toString(encoding)`-equivalent); nothing is ever held back, because
  none of these three encodings has a concept of an "incomplete" trailing
  unit: `latin1` maps every byte 1:1 to a code point 0–255, `ascii` maps
  every byte to `byte & 0x7F` (7-bit truncation — **not** an error/loss
  signal, silently drops the 8th bit), and `hex` maps every byte to exactly
  2 hex characters regardless of chunk boundaries.
- `end(buf?)` for these three is exactly `buf ? write(buf) : ''` — there is
  never anything left to flush.

### String input to `write()`/`end()`

Per the documented signature, `buffer` may itself be a `string` (not just
byte containers). Node re-encodes it via an implicit `Buffer.from(buffer)`
— which uses **UTF-8**, regardless of the decoder's own configured target
`encoding` — before running it through the normal decode path. This is a
real, if unusual, documented input form and must not be special-cased away.

### Reusability after `end()`

After `end()` runs (with or without a final `buffer` argument), all pending
state (`lastNeed`/`lastTotal`/scratch buffer) is reset to empty — the same
instance can immediately begin a fresh `write()`/`end()` sequence with no
leftover contamination from the previous one.

### Platform differences

None. This module is pure, deterministic, byte-level logic — no OS syscalls,
no filesystem, no network. There is no Windows vs POSIX distinction anywhere
in its behavior.

### Error/errno codes

| Code | Raised by | Meaning |
|---|---|---|
| `ERR_UNKNOWN_ENCODING` (`TypeError`) | `new StringDecoder(encoding)` | `encoding` is not a value `Buffer.isEncoding()` accepts. |

No other method in this module throws under documented usage; malformed
byte input always degrades to U+FFFD rather than throwing.

### Deprecations

None. No method in the current documented surface is deprecated.

### Security notes

- Decoding untrusted bytes never throws and never crashes — worst case is
  silent replacement-character substitution, which is the intended,
  spec-faithful behavior (this is a feature, not a bug: it prevents an
  attacker from using malformed UTF-8 to crash a decoding pipeline).
  Consumers that need to detect "this input contained invalid bytes" must
  check for U+FFFD in the output themselves — `string_decoder` does not
  expose an error signal.
- Because U+FFFD substitution is silent, chaining `string_decoder` in front
  of something security-sensitive (e.g. a path or command that trusts the
  decoded string) can mask truncation/corruption. Not this module's
  responsibility to fix — worth a one-line implementation-notes callout
  only, not a behavior change.

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node` is fully independent — no `rts-std` dependency. This module is
pure, allocation-light, synchronous computation; it needs no external crate
beyond what's already pulled in for `Buffer`/base64 support elsewhere in
`rts-node`.

| Surface area | Backing |
|---|---|
| `utf8` incomplete/invalid detection | `std::str::from_utf8(bytes)` — its `Err(Utf8Error)` gives `valid_up_to()` (the correctly-decoded prefix length) and `error_len()` (`None` ⇒ genuinely incomplete trailing sequence, hold it back; `Some(n)` ⇒ invalid bytes, emit U+FFFD immediately and resume after them). This maps almost directly onto Node's `utf8CheckIncomplete`/`utf8CheckExtraBytes` state machine without hand-rolling continuation-byte bit tests. |
| `utf16le`/`ucs2` | Hand-rolled: pair consecutive bytes into `u16` (little-endian), track odd trailing byte and trailing high-surrogate (`0xD800..=0xDBFF`) deferral exactly as described in §4. `char::decode_utf16` (Rust std) is a good building block for the "combine surrogate pair, or emit U+FFFD for a lone one" step once code units are assembled. |
| `base64`/`base64url` | The same base64 crate/engine `rts-node` already needs for `Buffer`'s own base64 support (a `base64` crate `Engine` with the standard and URL-safe-no-pad alphabets). The 3-byte grouping/remainder-holding logic is `rts-node`-local bookkeeping around calls to that engine, not something the crate provides itself. |
| `ascii`/`latin1`/`binary` | Trivial per-byte map: `latin1` → `byte as char` (every byte 0–255 is a valid Unicode scalar value, direct injection into the `String`); `ascii` → `(byte & 0x7F) as char`. No crate needed. |
| `hex` | `byte → 2 lowercase hex chars`; either hand-rolled or the same hex-encode helper `rts-node`'s crypto/buffer modules already need. |

**Handle storage**: a new small internal struct, e.g.
`NodeStringDecoderEntry { encoding: DecoderEncoding, pending: [u8; 4], pending_len: u8, needed: u8 }`
(`DecoderEncoding` = `Utf8 | Utf16Le | Base64 | Base64Url | Ascii | Latin1 |
Hex`), stored as a new `rts-node`-owned handle-table entry (`rts-node` needs
its *own* small handle table or a reusable slot in `rts-engine`'s generic
`HandleTable`'s free-form entry set — **not** the `rts-std`
`Entry::PromiseAsync`/etc. variants). This is a tiny, self-contained
per-instance struct — no `Arc<Mutex<_>>` needed since a given `StringDecoder`
JS object is used from a single JS-visible call site at a time (not shared
concurrently the way a socket handle can be polled from a background
thread).

### 5.2 ABI surface

`ns_prefix = "node_string_decoder"`, `node_module = "string_decoder"`,
registered in `rts-node`'s `NODE_SPECS` the same way as `fs`/`process`/`os`/
`path`/`util`/`crypto` today. The decoder instance is an opaque `Handle`; all
option/overload handling (accepting a `string` input by re-encoding it to
UTF-8 first, accepting `TypedArray`/`DataView` by resolving to the
underlying `ArrayBuffer` pointer + byte range) lives in a `.ts` shim — the
externs below are the raw byte-in/string-out primitive surface only.

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_STRING_DECODER_CREATE` | `StrPtr(encoding)` | `Handle` | Normalizes + validates the encoding name natively; on an unrecognized name returns the sentinel handle `0` and the `.ts` shim raises `ERR_UNKNOWN_ENCODING` (mirrors the existing RTS convention of a native status sentinel + `.ts`-side `Error` construction, e.g. as used for `fs`'s error-code mapping). |
| `__RTS_FN_NODE_STRING_DECODER_ENCODING` | `Handle` | `StrPtr` | Returns the normalized encoding name (`'utf8'`, `'utf16le'`, …) — a static Rust `&'static str`, safe to hand back directly as `StrPtr` (no GC allocation needed). |
| `__RTS_FN_NODE_STRING_DECODER_WRITE` | `Handle, U64(ptr), I64(len)` | `Handle` | `ptr`/`len` address the input byte range (already-normalized to raw bytes by the `.ts` shim — see §5.5). Returns a **GC string handle** (dynamically-computed, variable-length output; per the machine-ABI convention "dynamic strings are GC-allocated and return a `u64` handle read via `gc::string_ptr`/`gc::string_len`", the same mechanism every other computed-string-returning native fn in the runtime already uses). |
| `__RTS_FN_NODE_STRING_DECODER_END` | `Handle, Bool(has_buf), U64(ptr), I64(len)` | `Handle` | Same GC-string-handle return convention as `WRITE`. `has_buf = false` ⇒ `ptr`/`len` are ignored (equivalent to Node's `end()` with no argument). |
| `__RTS_FN_NODE_STRING_DECODER_FREE` | `Handle` | `Void` | Releases the decoder's handle-table slot. Called from the `.ts` shim's `FinalizationRegistry` cleanup or an explicit dispose path (Node itself has no explicit "close" — RTS's GC-backed handle can be freed opportunistically; see §5.7). |
| `__RTS_FN_NODE_STRING_DECODER_LAST_NEED` | `Handle` | `I32` | Legacy/undocumented `lastNeed` getter — phase (d), optional. |
| `__RTS_FN_NODE_STRING_DECODER_LAST_TOTAL` | `Handle` | `I32` | Legacy/undocumented `lastTotal` getter — phase (d), optional. |
| `__RTS_FN_NODE_STRING_DECODER_LAST_CHAR_LEN` / `_BYTE` | `Handle` / `Handle, I32(index)` | `I32` / `I32` | Legacy/undocumented `lastChar` accessor, exposed byte-at-a-time rather than as a full `Buffer` handle to avoid allocating a `Buffer` object for a rarely-used debug getter — phase (d), optional. |

No async, no callback, no Promise anywhere in this ABI — every symbol above
is a plain synchronous call.

### 5.3 Async model

Fully synchronous. `StringDecoder` has no async surface whatsoever — no
callback parameter, no Promise return, no event. `write()`/`end()` are pure
CPU-bound transforms over an in-memory byte range; there is nothing here for
the event loop, the tokio runtime, or the Promise subsystem to do. This is
the simplest possible async-model entry of any `node:*` module RTS will
implement.

### 5.4 Multithread / worker interaction

Per `docs/specs/rts-threading-model.md`:

- A `StringDecoder` instance is small, self-contained, pure-data state (an
  encoding tag + up to 4 pending bytes) — there is no OS resource, no shared
  mutable buffer beyond the instance's own scratch bytes, and no reason for
  two threads to ever touch the *same* instance concurrently (exactly
  mirroring how Node itself expects one `StringDecoder` per logical decode
  stream, never shared across `Worker` threads either — Node does not make
  `StringDecoder` transferable).
- Recommended mapping: **per-thread-local** ownership. Each RTS
  thread/region that needs to decode a byte stream constructs its own
  `StringDecoder`; the underlying handle-table slot is accessed only by its
  owning thread. `rts-engine`'s `HandleTable` being shard-aware/thread-safe
  is a safety net (no crash if misused across threads), not an invitation to
  actually share one decoder's pending-byte state between two threads —
  doing so would silently corrupt whichever thread's incomplete multi-byte
  sequence is in flight, precisely because the internal state is a single
  mutable scratch buffer, not a queue.
- No special multithread machinery (channel, shared-heap promotion) is
  needed for this module — it's a pure value-in/value-out computation, the
  simplest possible case in the threading model's spectrum.

### 5.5 Buffer / TypedArray interop

Input crosses the ABI as a raw `(ptr: u64, len: i64)` byte range, resolved
by the `.ts` shim from whichever accepted input form was passed:

- `Buffer`/`Uint8Array`/other `TypedArray`: use the array's own
  `buffer`/`byteOffset`/`byteLength` to compute the stable pointer + length
  directly (same `ArrayBuffer` pointer bridge every other byte-oriented
  `rts-node` module uses, per the primordial `ArrayBuffer`/`Buffer` memory
  model — `Buffer extends Uint8Array`).
- `DataView`: same underlying `ArrayBuffer`, via its own `byteOffset`/
  `byteLength`.
- `string` input (the documented but unusual case, §4): the `.ts` shim
  first UTF-8-encodes it into a scratch `Buffer` (the same encode step
  `TextEncoder` performs) *regardless of the decoder's configured target
  encoding* — matching Node's own implicit `Buffer.from(buffer)` — and then
  passes that scratch buffer's pointer through like any other byte input.

Output never crosses as raw bytes — the whole point of this module is
bytes→string decoding, so `WRITE`/`END` always return a GC string handle
(§5.2), which the `.ts` shim's calling convention already knows how to turn
into a JS `string` value (the same mechanism used by every other
dynamically-computed string-returning native function in the runtime).

### 5.6 Doctrine placement

`node:string_decoder` is unambiguously **non-primordial** — it has no
native literal syntax; a decoder is reached only via `new StringDecoder()`
after an explicit `import`/`require`. The engine must never hardcode the
string `"string_decoder"` (or `"StringDecoder"`) anywhere in
`crates/rts-codegen-new/`. Resolution path: `import { StringDecoder } from
"node:string_decoder"` → `ns_prefix_for("node:string_decoder")` →
`"node_string_decoder"` → `node_lookup("node_string_decoder.<member>")` → the
matching `NodespaceMember` in `rts-node`'s `string_decoder::SPEC` — exactly
the same "registry for node:" data-table pattern already used by
`node:dgram`/`node:fs`/`node:process`/`node:util`. Adding this module means
adding one new `NodespaceSpec` entry to `NODE_SPECS`, never touching engine
control flow. All JS-facing ergonomics (the `StringDecoder` class shape,
constructor overload/encoding-name normalization, throwing
`ERR_UNKNOWN_ENCODING`, the `string` input UTF-8 re-encode step, the
legacy `lastChar`/`lastNeed`/`lastTotal` getters) live entirely in a `.ts`
shim shipped by `rts-node`; only the raw primitive ops in §5.2 are native
`extern "C"` symbols.

### 5.7 Shared-infra dependencies (FLAG)

**None.** This is the rare `node:*` module with essentially zero
cross-cutting infra needs:

- No tokio runtime, no event loop, no Promise subsystem — everything is
  synchronous, in-process, single-call computation (§5.3).
- The only two primitives it touches — the GC string-handle allocator
  (`gc::string_from_*`-equivalent) and the `ArrayBuffer`/`Buffer` stable
  byte-pointer bridge — already live in `rts-engine` as part of the
  **primordial** value/memory model (not `rts-std`), so there is no
  "currently in `rts-std`, needs hoisting" conflict here at all, unlike
  most other `node:*` modules surveyed so far (`dgram`, `net`, `fs.watch`,
  etc.) that need the shared tokio runtime or the event-loop keep-alive
  extension.
- No `EventEmitter` base needed (this class emits nothing), so the
  cross-module "which `EventEmitter` base do bespoke `.ts` shims share"
  question (flagged in `dgram.md` §7) does not apply here either.

### 5.8 Implementation phases

a. **`utf8` core** — `CREATE`/`WRITE`/`END`/`FREE` externs for the `utf8`
   encoding only, built on `std::str::from_utf8`'s `valid_up_to`/
   `error_len` split (§5.1). Enough for the majority of real-world usage
   (most stream `encoding` options default to/use `'utf8'`).
b. **"Simple" encodings** — `ascii`, `latin1`/`binary`, `hex`: no new
   externs needed beyond dispatching on the stored `DecoderEncoding` tag
   inside the same `WRITE`/`END` — trivial per-byte transforms, no
   buffering path at all.
c. **`base64`/`base64url`** — 3-byte-group buffering logic (§4) around the
   shared base64 engine `rts-node` already needs for `Buffer`.
d. **`utf16le`/`ucs2`** — 2-byte code-unit buffering + trailing
   high-surrogate deferral (§4), using `char::decode_utf16` for the
   final surrogate-combine/U+FFFD-substitute step.
e. **Legacy/undocumented compat surface** — `lastChar`/`lastNeed`/
   `lastTotal` getters + the `text(buf, offset)` prototype method, only if
   a concrete consumer (a vendored `readable-stream` shim, or RTS's own
   future `fs`/`net`/`zlib` `'encoding'`-option consumers) is found to need
   direct access rather than going through `write()`/`end()` normally.
f. **`.ts` shim** — the `StringDecoder` class wrapping the handle:
   constructor encoding validation/normalization + `ERR_UNKNOWN_ENCODING`
   throw, `write`/`end` argument-form normalization (string → UTF-8
   `Buffer` re-encode per §4/§5.5), and instance disposal wiring (§5.7 —
   likely a `FinalizationRegistry` entry since Node exposes no explicit
   `close()`/`dispose()` on this class).
g. **Test fixtures** (§6).

## 6. Test plan

1. **Default encoding**: `new StringDecoder()` behaves identically to
   `new StringDecoder('utf8')`; `write()` of a plain ASCII string round-trips
   exactly.
2. **Whole-chunk UTF-8**: `write(Buffer.from('héllo wörld 😀'))` in one call
   returns the exact original string (no splitting involved).
3. **UTF-8 split across every possible boundary of a 4-byte character**:
   encode `'😀'` (bytes `F0 9F 98 80`) and split the two `write()` calls
   after byte 1, byte 2, and byte 3 (three separate sub-tests); assert each
   first `write()` returns `''` (or any preceding whole characters, nothing
   from the split one) and the second `write()` returns the correctly
   reassembled `'😀'`.
4. **UTF-8 invalid continuation byte**: feed a lead byte followed by a
   non-continuation byte (e.g. `C2 41`); assert immediate `'�' + 'A'`
   rather than waiting/hanging for more bytes.
5. **UTF-8 `end()` flush of a never-completed sequence**: `write()` a lone
   lead byte (e.g. just `0xE2`, expecting 3 total), never send the rest,
   call `end()`; assert exactly one U+FFFD is returned (not one per
   buffered byte).
6. **UTF-16LE split on an odd byte**: `write()` an odd number of bytes of a
   `'utf16le'`-encoded string, assert the dangling byte is held and
   combined correctly on the next `write()`.
7. **UTF-16LE split exactly at a surrogate pair**: encode an astral
   character (e.g. `'𝌆'`, U+1D306) as `'utf16le'`, split the `write()` calls
   exactly between the high-surrogate code unit and the low-surrogate code
   unit; assert the second `write()` reassembles the correct single
   character (not two separate lone-surrogate replacement characters).
8. **UTF-16LE `end()` with a dangling lone high surrogate**: `write()` only
   the high-surrogate code unit's 2 bytes, call `end()` with nothing more;
   assert exactly one U+FFFD.
9. **`base64` buffering across a non-multiple-of-3 split**: `write()` 4
   bytes in two calls (3 then 1); assert the first call's output is exactly
   the base64 of the first 3 bytes (4 chars), the second call/`end()`
   correctly flushes the remaining 1 byte with `==` padding, and the full
   concatenation equals `Buffer.from(...).toString('base64')` on the whole
   original input.
10. **`base64url` — same split scenario, no padding**: assert the flushed
    remainder has zero `=` characters and uses `-`/`_` in place of `+`/`/`
    where applicable.
11. **`ascii`/`latin1`/`hex` — no buffering, byte-for-byte parity**: for
    each of the three, `write()` various chunk splits of the same input
    and assert the concatenated output exactly equals a single whole-buffer
    `toString(encoding)` call (proving no state leaks/holds bytes across
    calls for these encodings).
12. **Invalid encoding name**: `new StringDecoder('utf-9')` throws
    `TypeError` with an `ERR_UNKNOWN_ENCODING`-shaped message.
13. **Reuse after `end()`**: call `end()`, then start a fresh
    `write()`/`write()`/`end()` sequence on the *same* instance; assert no
    contamination from the previous sequence's leftover bytes.
14. **String input to `write()`/`end()`**: pass a JS `string` (not a
    `Buffer`) directly; assert it is treated as UTF-8 bytes of that string
    regardless of the decoder's own configured encoding (e.g. a
    `'base64'`-configured decoder fed a string argument still UTF-8-encodes
    the string first, per §4, before applying base64 grouping to those
    bytes).
15. **Empty input**: `write(Buffer.alloc(0))` returns `''`; `end()` with no
    prior `write()` and no argument returns `''`.
16. **Legacy getters** (if implemented, phase e): after a partial UTF-8
    `write()` that holds back 2 bytes of a 3-byte sequence, assert
    `lastNeed === 1`, `lastTotal === 3`, and the pending bytes are
    retrievable.
17. **Multithread isolation**: construct independent `StringDecoder`
    instances on N separate RTS worker threads (per the threading model),
    each decoding a different interleaved-chunk UTF-8 stream concurrently;
    assert no cross-thread contamination of any instance's pending-byte
    state (each instance's correctness must be independent of what any
    other thread's decoder is doing).

## 7. Open questions / deferrals

- **Legacy `lastChar`/`lastNeed`/`lastTotal`/`text()` compat** (§5.8 phase
  e) — undocumented Node implementation details; defer full native support
  until a concrete RTS-side consumer (a vendored stream-compat shim) is
  shown to need direct access rather than going through `write()`/`end()`.
- **Streams integration** — Node's own `fs.createReadStream({encoding})`,
  `net.Socket.setEncoding()`, and `zlib` streams all internally construct a
  `StringDecoder` to implement their `'encoding'` option. When those
  modules' specs are written, they should reference and reuse this native
  surface (via the same `.ts`-shim pattern) rather than reimplementing
  chunk-boundary-safe decoding from scratch.
- **Base64 malformed-input leniency** — Node's `Buffer.from(str, 'base64')`
  is lenient (skips invalid characters) rather than throwing; whether
  `StringDecoder`'s base64 path needs to mirror that exact leniency (as
  opposed to simply consuming valid-looking groups) should be verified
  against `Buffer`'s own base64 decode semantics once that surface is
  speced, to avoid two subtly-different base64 decoders in the same
  crate.
- **Handle-table placement** (§5.1) — whether `rts-node` gets its own
  dedicated small `HandleTable`-like slab for lightweight non-OS-resource
  handles (like this decoder's tiny struct) versus reusing a generic
  `rts-engine` facility, is a cross-module decision that should be made
  once alongside the first 2–3 `rts-node` modules that need a
  non-OS-resource opaque handle (this module is a good minimal example to
  settle it with, given it has zero other complexity).
