# node:zlib

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:zlib` |
| Node.js version | 25.x |
| Stability | 2 - Stable (the Zstd sub-surface — `Zstd*` classes/functions/constants — is separately marked **1 - Experimental**) |
| Tier | P1 |
| Status | [ ] Not implemented — spec only |
| Import forms | `import zlib from "node:zlib"`; `import { gzipSync, createGzip, constants, ... } from "node:zlib"`; `import zlib from "zlib"` (bare specifier, legacy alias); `const zlib = require("node:zlib")` / `require("zlib")` |
| Globals exposed | None. `node:zlib` introduces no `globalThis` member — everything is reached through the module's named/default export. |

## 1. Purpose

`node:zlib` exposes DEFLATE/zlib, gzip, Brotli, and (experimentally) Zstandard
compression and decompression to JS/TS, in three equivalent shapes: one-shot
buffer-to-buffer convenience functions (sync and callback), incremental
`stream.Transform` classes for piping arbitrarily large data, and a shared
`constants` namespace mirroring the underlying C library's tuning knobs
(flush modes, compression levels/strategies, Brotli/Zstd parameters). It is
the substrate `node:http`/`node:https` content-encoding negotiation is built
on (`Content-Encoding: gzip|deflate|br|zstd`), and is used standalone for
ad hoc buffer/file compression.

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `zlib.ZlibBase` (internal base; not directly exported/constructible)

Base class: `stream.Transform`. Renamed from `zlib.Zlib` in v11.7.0 — the
old name is gone, not kept as a deprecated alias. Every class below extends
it and inherits its members.

| Member | Kind | Signature | Returns | Notes |
|---|---|---|---|---|
| `zlibBase.bytesWritten` | property (readonly) | `number` | — | Bytes written to the underlying engine before processing (i.e. input bytes consumed so far). |
| `zlibBase.close` | instance method | `(callback?: () => void) => void` | `void` | Closes the underlying native handle; releases native memory. Safe to call multiple times. |
| `zlibBase.flush` | instance method | `(kind?: number, callback?: () => void) => void` | `void` | `kind` default: `Z_FULL_FLUSH` (zlib-based classes), `BROTLI_OPERATION_FLUSH` (Brotli), `ZSTD_e_flush` (Zstd). Queues a flush behind pending writes; does not force immediate stream-level output. |
| `zlibBase.params` | instance method | `(level: number, strategy: number, callback: () => void) => void` | `void` | **Zlib-deflate-family only** (`Deflate`/`DeflateRaw`). Dynamically changes compression level/strategy mid-stream. Not applicable to Inflate/Gunzip/Unzip/Brotli/Zstd. |
| `zlibBase.reset` | instance method | `() => void` | `void` | **Inflate and Deflate (zlib-based) only.** Resets the compressor/decompressor to its initial state without reallocating. Not applicable to Brotli/Zstd/Gzip/Gunzip. |

Events (inherited from `stream.Transform`/`Duplex`/`Readable`/`Writable`):
`'data'`, `'end'`, `'error'`, `'close'`, `'drain'`, `'finish'`, `'pipe'`,
`'unpipe'`. Class-specific error-event triggers are listed per class below.

#### `zlib.Deflate`

Base class: `ZlibBase`. Compresses data using the DEFLATE algorithm with a
zlib header/trailer (RFC 1950). Constructed via `zlib.createDeflate(options?)`.
Supports `params()`, `reset()`, `dictionary` option.

#### `zlib.Inflate`

Base class: `ZlibBase`. Decompresses a zlib-wrapped DEFLATE stream (RFC 1950).
Constructed via `zlib.createInflate(options?)`. Supports `reset()`,
`dictionary` option. Emits `'error'` on a truncated input stream (since v5.0.0).

#### `zlib.DeflateRaw`

Base class: `ZlibBase`. Compresses using raw DEFLATE (RFC 1951, no zlib
header/trailer). Constructed via `zlib.createDeflateRaw(options?)`. Supports
`params()`, `reset()`, `dictionary` option.

#### `zlib.InflateRaw`

Base class: `ZlibBase`. Decompresses raw DEFLATE data (RFC 1951). Constructed
via `zlib.createInflateRaw(options?)`. Supports `reset()`, `dictionary` option.

#### `zlib.Gzip`

Base class: `ZlibBase`. Compresses using gzip framing (RFC 1952: magic bytes,
OS/mtime/name header fields, CRC32 + size trailer). Constructed via
`zlib.createGzip(options?)`. Supports `params()`, no `dictionary` support
(gzip format has none), no `reset()`.

#### `zlib.Gunzip`

Base class: `ZlibBase`. Decompresses gzip-framed data. Constructed via
`zlib.createGunzip(options?)`. Emits `'error'` on trailing garbage after a
valid gzip member (since v6.0.0) and on a truncated stream.

#### `zlib.Unzip`

Base class: `ZlibBase`. Auto-detects gzip vs zlib-wrapped-deflate framing by
inspecting the header of the first bytes and dispatches to the matching
decoder. Constructed via `zlib.createUnzip(options?)`.

#### `zlib.BrotliCompress`

Base class: `ZlibBase`. Compresses using the Brotli algorithm (RFC 7932).
Constructed via `zlib.createBrotliCompress(options?)`. Does not support
`params()`/`reset()`/`dictionary` (uses a `params` map instead — see §3);
its `flush()`/options use `BROTLI_OPERATION_*` constants, not `Z_*`.

#### `zlib.BrotliDecompress`

Base class: `ZlibBase`. Decompresses Brotli-compressed data. Constructed via
`zlib.createBrotliDecompress(options?)`.

#### `zlib.ZstdCompress` (Experimental — Stability: 1)

Base class: `ZlibBase`. Compresses using Zstandard. Constructed via
`zlib.createZstdCompress(options?)`. Added v23.8.0. Uses `ZSTD_*` constants
for flush/params; supports a `dictionary` option (`Buffer`) and a
`pledgedSrcSize` tuning hint.

#### `zlib.ZstdDecompress` (Experimental — Stability: 1)

Base class: `ZlibBase`. Decompresses Zstandard-compressed data. Constructed
via `zlib.createZstdDecompress(options?)`. Added v23.8.0.

### 2.2 Top-level functions

**Stream factory functions** (11) — each is a thin `new XClass(options)`
wrapper:

| Signature | Returns | Variant |
|---|---|---|
| `zlib.createDeflate(options?: ZlibOptions) => Deflate` | `Deflate` | sync (returns a stream immediately; work is async internally) |
| `zlib.createInflate(options?: ZlibOptions) => Inflate` | `Inflate` | sync |
| `zlib.createDeflateRaw(options?: ZlibOptions) => DeflateRaw` | `DeflateRaw` | sync |
| `zlib.createInflateRaw(options?: ZlibOptions) => InflateRaw` | `InflateRaw` | sync |
| `zlib.createGzip(options?: ZlibOptions) => Gzip` | `Gzip` | sync |
| `zlib.createGunzip(options?: ZlibOptions) => Gunzip` | `Gunzip` | sync |
| `zlib.createUnzip(options?: ZlibOptions) => Unzip` | `Unzip` | sync |
| `zlib.createBrotliCompress(options?: BrotliOptions) => BrotliCompress` | `BrotliCompress` | sync |
| `zlib.createBrotliDecompress(options?: BrotliOptions) => BrotliDecompress` | `BrotliDecompress` | sync |
| `zlib.createZstdCompress(options?: ZstdOptions) => ZstdCompress` | `ZstdCompress` | sync (experimental) |
| `zlib.createZstdDecompress(options?: ZstdOptions) => ZstdDecompress` | `ZstdDecompress` | sync (experimental) |

**Convenience buffer functions** (11 algorithms × sync/callback = 22). Each
async form's full signature; the `*Sync` form drops the `callback` param and
returns `Buffer` directly (throwing instead of calling back with an error).

| Function | Params | Returns / callback result | Throws (`*Sync`) | Variant |
|---|---|---|---|---|
| `zlib.deflate(buffer, options?, callback)` | `buffer: InputType`; `options?: ZlibOptions`; `callback: (error: Error \| null, result: Buffer) => void` | via callback | `Error` (`ERR_INVALID_ARG_TYPE`, `ERR_BUFFER_TOO_LARGE`, zlib data errors) | callback |
| `zlib.deflateSync(buffer, options?)` | `buffer: InputType`; `options?: ZlibOptions` | `Buffer` | same | sync |
| `zlib.inflate(buffer, options?, callback)` | same shape | via callback | same, plus `Z_DATA_ERROR`-class errors on malformed input | callback |
| `zlib.inflateSync(buffer, options?)` | same shape | `Buffer` | same | sync |
| `zlib.deflateRaw(buffer, options?, callback)` | same shape | via callback | same | callback |
| `zlib.deflateRawSync(buffer, options?)` | same shape | `Buffer` | same | sync |
| `zlib.inflateRaw(buffer, options?, callback)` | same shape | via callback | same | callback |
| `zlib.inflateRawSync(buffer, options?)` | same shape | `Buffer` | same | sync |
| `zlib.gzip(buffer, options?, callback)` | same shape | via callback | same | callback |
| `zlib.gzipSync(buffer, options?)` | same shape | `Buffer` | same | sync |
| `zlib.gunzip(buffer, options?, callback)` | same shape | via callback | same, plus trailing-garbage/truncation errors | callback |
| `zlib.gunzipSync(buffer, options?)` | same shape | `Buffer` | same | sync |
| `zlib.unzip(buffer, options?, callback)` | same shape | via callback | same | callback |
| `zlib.unzipSync(buffer, options?)` | same shape | `Buffer` | same | sync |
| `zlib.brotliCompress(buffer, options?: BrotliOptions, callback)` | same shape (options is `BrotliOptions`) | via callback | same | callback |
| `zlib.brotliCompressSync(buffer, options?: BrotliOptions)` | same shape | `Buffer` | same | sync |
| `zlib.brotliDecompress(buffer, options?: BrotliOptions, callback)` | same shape | via callback | same | callback |
| `zlib.brotliDecompressSync(buffer, options?: BrotliOptions)` | same shape | `Buffer` | same | sync |
| `zlib.zstdCompress(buffer, options?: ZstdOptions, callback)` (experimental) | same shape | via callback | same | callback |
| `zlib.zstdCompressSync(buffer, options?: ZstdOptions)` (experimental) | same shape | `Buffer` | same | sync |
| `zlib.zstdDecompress(buffer, options?: ZstdOptions, callback)` (experimental) | same shape | via callback | same | callback |
| `zlib.zstdDecompressSync(buffer, options?: ZstdOptions)` (experimental) | same shape | `Buffer` | same | sync |

Where `InputType = Buffer | TypedArray | DataView | ArrayBuffer | string`
(a `string` input is UTF-8-encoded; there is no `inputEncoding` parameter).

**Other top-level functions** (1):

| Signature | Params | Returns | Throws | Variant |
|---|---|---|---|---|
| `zlib.crc32(data, value?)` | `data: string \| Buffer \| TypedArray \| DataView`; `value?: number` (starting CRC, default `0`, must fit an unsigned 32-bit integer) | `number` (unsigned 32-bit CRC32 checksum) | `ERR_INVALID_ARG_TYPE` (bad `data` type); `ERR_OUT_OF_RANGE` (bad `value`) | sync |

**Total top-level functions documented: 34** (11 factories + 22 convenience
+ 1 `crc32`).

### 2.3 Properties & constants

| Name | Type | Notes |
|---|---|---|
| `zlib.constants` | `object` (frozen) | Namespace holding every constant below. Historically some were also reachable directly on `zlib.*` — that direct form is **deprecated**; always use `zlib.constants.*`. |

**Zlib flush values** (`zlib.constants.*`): `Z_NO_FLUSH`, `Z_PARTIAL_FLUSH`,
`Z_SYNC_FLUSH`, `Z_FULL_FLUSH`, `Z_FINISH`, `Z_BLOCK`.

**Zlib return/error codes**: `Z_OK`, `Z_STREAM_END`, `Z_NEED_DICT`,
`Z_ERRNO`, `Z_STREAM_ERROR`, `Z_DATA_ERROR`, `Z_MEM_ERROR`, `Z_BUF_ERROR`,
`Z_VERSION_ERROR`.

**Zlib compression levels**: `Z_NO_COMPRESSION`, `Z_BEST_SPEED`,
`Z_BEST_COMPRESSION`, `Z_DEFAULT_COMPRESSION`.

**Zlib compression strategy**: `Z_FILTERED`, `Z_HUFFMAN_ONLY`, `Z_RLE`,
`Z_FIXED`, `Z_DEFAULT_STRATEGY`.

**Brotli flush operations**: `BROTLI_OPERATION_PROCESS` (default for all
operations), `BROTLI_OPERATION_FLUSH` (default for `.flush()`),
`BROTLI_OPERATION_FINISH` (default for the final chunk),
`BROTLI_OPERATION_EMIT_METADATA` (not practically usable via Node's stream
layer — see §4).

**Brotli compressor parameters**: `BROTLI_PARAM_MODE` (with
`BROTLI_MODE_GENERIC` default / `BROTLI_MODE_TEXT` / `BROTLI_MODE_FONT`),
`BROTLI_PARAM_QUALITY` (with `BROTLI_MIN_QUALITY` / `BROTLI_MAX_QUALITY` /
`BROTLI_DEFAULT_QUALITY`), `BROTLI_PARAM_SIZE_HINT`, `BROTLI_PARAM_LGWIN`
(with `BROTLI_MIN_WINDOW_BITS` / `BROTLI_MAX_WINDOW_BITS` /
`BROTLI_DEFAULT_WINDOW` / `BROTLI_LARGE_MAX_WINDOW_BITS`),
`BROTLI_PARAM_LGBLOCK` (with `BROTLI_MIN_INPUT_BLOCK_BITS` /
`BROTLI_MAX_INPUT_BLOCK_BITS`), `BROTLI_PARAM_DISABLE_LITERAL_CONTEXT_MODELING`,
`BROTLI_PARAM_LARGE_WINDOW`, `BROTLI_PARAM_NPOSTFIX`, `BROTLI_PARAM_NDIRECT`,
`BROTLI_MAX_NPOSTFIX`.

**Brotli decompressor parameters**:
`BROTLI_DECODER_PARAM_DISABLE_RING_BUFFER_REALLOCATION`,
`BROTLI_DECODER_PARAM_LARGE_WINDOW`.

**Zstd flush operations** (experimental): `ZSTD_e_continue` (default for all
operations), `ZSTD_e_flush` (default for `.flush()`), `ZSTD_e_end` (default
for the final chunk).

**Zstd compressor parameters** (experimental): `ZSTD_c_compressionLevel`,
`ZSTD_c_strategy` (with strategy values `ZSTD_fast`, `ZSTD_dfast`,
`ZSTD_greedy`, `ZSTD_lazy`, `ZSTD_lazy2`, `ZSTD_btlazy2`, `ZSTD_btopt`,
`ZSTD_btultra`, `ZSTD_btultra2`).

**Zstd decompressor parameters** (experimental): `ZSTD_d_windowLogMax`.

### 2.4 Events

All `ZlibBase`-derived classes emit the standard `stream.Transform` event
set: `'data'`, `'end'`, `'error'`, `'close'`, `'drain'`, `'finish'`, `'pipe'`,
`'unpipe'`. No zlib-specific event names are added; class-specific
**conditions** that raise `'error'` are noted per class in §2.1 (gunzip
trailing garbage, inflate truncation, data/version/memory errors surfaced
from the underlying codec).

## 3. Types & option objects

```typescript
type InputType = Buffer | NodeJS.TypedArray | DataView | ArrayBuffer | string;

type CompressCallback = (error: Error | null, result: Buffer) => void;

interface ZlibOptions {
  flush?: number;             // default: constants.Z_NO_FLUSH
  finishFlush?: number;       // default: constants.Z_FINISH
  chunkSize?: number;         // default: 16 * 1024
  windowBits?: number;        // default: 15 (9-15); DeflateRaw/InflateRaw use it without the zlib wrapper
  level?: number;             // compression only: -1 (Z_DEFAULT_COMPRESSION) .. 9
  memLevel?: number;          // compression only: 1..9, default 8
  strategy?: number;          // compression only: one of the Z_* strategy constants
  dictionary?: Buffer | NodeJS.TypedArray | DataView | ArrayBuffer; // Deflate/DeflateRaw/Inflate/InflateRaw only (not Gzip/Gunzip/Unzip)
  info?: boolean;             // if true, callback/return becomes { buffer: Buffer, engine: Zlib } instead of a bare Buffer
  maxOutputLength?: number;   // default: buffer.kMaxLength (convenience functions only)
}

interface BrotliOptions {
  flush?: number;             // default: constants.BROTLI_OPERATION_PROCESS
  finishFlush?: number;       // default: constants.BROTLI_OPERATION_FINISH
  chunkSize?: number;         // default: 16 * 1024
  params?: {                  // keyed by BROTLI_PARAM_*/BROTLI_DECODER_PARAM_* constant -> integer value
    [key: number]: number;
  };
  maxOutputLength?: number;   // default: buffer.kMaxLength
  info?: boolean;             // default: false
}

interface ZstdOptions { // Experimental
  flush?: number;             // default: constants.ZSTD_e_continue
  finishFlush?: number;       // default: constants.ZSTD_e_end
  chunkSize?: number;         // default: 16 * 1024
  params?: {                  // keyed by ZSTD_c_*/ZSTD_d_* constant -> integer value
    [key: number]: number;
  };
  dictionary?: Buffer;        // compression and decompression
  pledgedSrcSize?: number;    // hint of total uncompressed size, compression only
  maxOutputLength?: number;   // default: buffer.kMaxLength
  info?: boolean;             // default: false
}

// Returned shape when `info: true` is passed to a convenience function:
interface ZlibInfoResult {
  buffer: Buffer;
  engine: ZlibBase; // the internal compressor/decompressor instance used for the one-shot call
}
```

## 4. Node semantics & edge cases

- **Truncated input.** By default, decompressing a truncated stream (missing
  final DEFLATE/gzip/Brotli block) throws/emits an `'error'`. This can be
  relaxed by passing `finishFlush: zlib.constants.Z_SYNC_FLUSH` (zlib-based)
  so partial data is still returned instead of erroring — used e.g. to
  tolerate a server that closes a gzip response early.
- **Gunzip trailing garbage** (bytes after a complete, valid gzip member)
  produces an `'error'` event (since v6.0.0). **Inflate on a truncated
  stream** produces an `'error'` event (since v5.0.0).
- **Memory tuning (zlib-based).** Deflate memory usage (bytes) ≈
  `(1 << (windowBits + 2)) + (1 << (memLevel + 9))`. Defaults
  (`windowBits: 15`, `memLevel: 8`) → 128 KiB + 128 KiB = 256 KiB. Reducing to
  `{ windowBits: 14, memLevel: 7 }` halves memory to ~128 KiB at the cost of
  compression ratio. Inflate memory ≈ `1 << windowBits` (32 KiB default) plus
  the internal output slab (`chunkSize`, default 16 KiB).
- **Level/strategy tradeoff.** Higher `level` = better ratio, slower; larger
  `memLevel`/`chunkSize` = fewer internal engine calls per write, faster but
  more memory. Brotli's `BROTLI_PARAM_QUALITY` and Zstd's
  `ZSTD_c_compressionLevel` are the analogous knobs; Brotli's
  `BROTLI_PARAM_LGWIN` and Zstd's window-log parameter are the analogs of
  zlib's `windowBits`.
- **Threadpool pressure.** Every zlib API except the `*Sync` methods runs on
  Node's internal (libuv) threadpool. Spinning up a very large number of
  concurrent one-shot calls (Node's own docs warn against, e.g., 30,000
  concurrent `zlib.deflate()` calls in a tight loop) causes severe memory
  fragmentation; callers are expected to cache compressed results rather than
  recompute per-request. This maps directly onto RTS's shared tokio runtime
  `spawn_blocking` pool (§5.3) — the same pressure characteristics apply.
- **`zlib.constants` is frozen** and is the only supported way to reach these
  values; direct `zlib.Z_NO_FLUSH`-style top-level access is deprecated
  legacy surface, not implemented as new surface.
- **Class rename**: the base class is `ZlibBase` (was `Zlib` prior to
  v11.7.0); the old name was removed, not deprecated-and-kept.
- **Zstd is experimental** (Stability: 1, added v23.8.0) — smaller ecosystem
  maturity than zlib/Brotli; Node itself may still change this surface.
- **`maxOutputLength`** bounds convenience-function output size (default
  `buffer.kMaxLength`, i.e. the platform's maximum `Buffer` size); exceeding
  it surfaces as an error before the full result is returned (exact error
  code text needs verification against a live Node build — see §7).
- **`crc32(data, value?)`** (added v22.2.0) computes a standalone CRC32 over
  arbitrary bytes — it is unrelated to any stream instance and does not
  require constructing a Gzip/Deflate object; passing `value` from a
  previous call chains the checksum across sequential chunks (matching
  zlib's native `crc32_combine`-free chunked usage pattern: seed the next
  call with the prior return value).
- **No platform (Windows vs POSIX) differences** — this module is pure
  in-memory codec work with no filesystem/path semantics; behavior is
  identical across platforms modulo whatever the underlying compiled codec
  library itself guarantees (bit-identical output is not itself a spec
  guarantee across zlib versions, only round-trip correctness is).
- **`util.promisify` friendliness.** The convenience functions
  (`gzip`/`gunzip`/`deflate`/.../`brotliCompress`/`brotliDecompress`/
  `zstdCompress`/`zstdDecompress`) follow the standard Node
  `(err, result) => void` callback shape and are commonly wrapped with
  `util.promisify()` in user code; `node:zlib` itself does not export a
  Promise-returning variant of these functions. (Cross-module note — see §7.)
- **Ordering/backpressure.** As `stream.Transform` instances, the streaming
  classes follow standard `Writable`/`Readable` backpressure: `.write()`
  returning `false` and waiting for `'drain'`, `.pipe()` handling flow
  control automatically. `.flush()` calls are queued behind pending writes,
  not applied immediately — calling `.flush()` too eagerly (e.g. after every
  byte) measurably degrades the compression ratio because it forces early
  block boundaries.
- **Security note.** Decompressing untrusted input with an attacker-chosen
  `windowBits`/`chunkSize`/no `maxOutputLength` cap is a classic "zip bomb"
  memory-exhaustion vector; `maxOutputLength` is the documented mitigation
  and should be set explicitly by API consumers handling untrusted
  compressed payloads (e.g. HTTP request bodies).

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node` owns this module end-to-end with its own vendored Rust crates —
it does **not** reuse any existing rts-std compression code (none currently
exists in rts-std under this name) and must not grow a dependency on
rts-std per the crate-independence decision.

| Area | Rust crate(s) | Notes |
|---|---|---|
| DEFLATE / zlib-wrapped deflate / gzip (`Deflate*`, `Inflate*`, `Gzip`, `Gunzip`) | `flate2` (with its pure-Rust `miniz_oxide` backend — no C toolchain dependency, matching the project's existing "no OpenSSL/schannel" stance for `tls`) | `flate2::write`/`read::{DeflateEncoder, DeflateDecoder}` for raw deflate; `Zlib{Encoder,Decoder}` for the zlib-wrapped form; `Gz{Encoder,Decoder}` for gzip framing. One-shot (`Vec<u8>` in/out) and incremental (`Compress`/`Decompress` struct, `.compress()`/`.decompress()` with a status enum) both available from the same crate. |
| `Unzip` auto-detection | Hand-rolled magic-byte sniff on top of `flate2` (peek first 2 bytes: `0x1f 0x8b` → gzip path; else zlib CMF/FLG header check `(byte0 * 256 + byte1) % 31 == 0` with valid CM/CINFO nibble → zlib path; else raw-deflate-with-auto-header per Node's own `windowBits: 32` libz trick) | `flate2` does not expose a single "auto" mode across gzip+zlib the way raw zlib's `inflateInit2(windowBits+32)` does; this is reimplemented as an explicit header sniff, functionally equivalent for the two supported framings (see §7 for the divergence risk on malformed/edge-case headers). |
| Brotli (`BrotliCompress`/`BrotliDecompress`) | `brotli` crate (pure Rust encoder **and** decoder, no C dependency) | Exposes both one-shot and streaming (`CompressorWriter`/`Decompressor`) APIs; quality/`lgwin`/mode map onto `BROTLI_PARAM_QUALITY`/`BROTLI_PARAM_LGWIN`/`BROTLI_PARAM_MODE`. |
| Zstandard (`ZstdCompress`/`ZstdDecompress`, experimental) | `zstd` crate (bindings to the C `libzstd` via `zstd-sys`, requires a C compiler at build time) as the pragmatic first cut; a pure-Rust decoder (`ruzstd`) exists but a mature pure-Rust **encoder** does not as of this writing | Tension with the project's general pure-Rust preference — flagged as a gap/decision point in §7. Given Node marks this surface Experimental (Stability 1) itself, it is the lowest-priority sub-area (§5.8j) and the crate choice can be revisited without blocking the rest of the module. |
| CRC32 (`zlib.crc32`) | `crc32fast` (runtime-detected SIMD, widely used, pure Rust) | Supports the seeded/chained form (`Hasher::new_with_initial(value)`) matching `crc32(data, value)`. |

### 5.2 ABI surface

Symbol convention: `__RTS_FN_NODE_ZLIB_<NAME>`, one typed `extern "C"` per
primitive operation. Rather than one native constructor per Node class (11
classes with near-identical option shapes), the native layer exposes a
single **kind-discriminated** constructor and a handful of shared
operation entry points — the discriminant is a plain data value passed at
the call site, not a name the engine hardcodes (consistent with the
primordial-vs-registry doctrine: the engine only ever sees
`node_zlib.createGzip` as a qualified member name resolved through
`NODE_SPECS`/`node_lookup`; which native symbol and which `kind` constant
that member maps to is a data-table decision made entirely inside
`rts-node`, never a codegen `match` on a module name).

**`kind` discriminant** (`I32`, native-internal — not part of the public
`.ts` surface): `0 = Deflate`, `1 = Inflate`, `2 = DeflateRaw`,
`3 = InflateRaw`, `4 = Gzip`, `5 = Gunzip`, `6 = Unzip`,
`7 = BrotliCompress`, `8 = BrotliDecompress`, `9 = ZstdCompress`,
`10 = ZstdDecompress`.

**Handle model.** Every live stream instance (`Deflate`/`Inflate`/.../
`ZstdDecompress`) is an opaque `u64` Handle into `rts-node`'s own private
sharded handle table (the same "own table, not a new `Entry` arm in
`rts-engine`'s shared `HandleTable`" pattern used elsewhere in `rts-node`,
e.g. `node:crypto`'s `Hash`/`Cipheriv` handles). Input/output byte payloads
are **not** copied through `StrPtr`; because `Buffer`/`Uint8Array` are
primordial TypedArray values, the native layer reads/writes the underlying
`ArrayBuffer` memory directly via the engine's existing
`arraybuffer_data_ptr`/`arraybuffer_byte_len` accessors (same reuse pattern
documented in `buffer.md` §5.5) — avoiding a double copy on what can be
multi-megabyte compression payloads.

| Symbol | Args (`AbiType`) | Returns | Maps to |
|---|---|---|---|
| `__RTS_FN_NODE_ZLIB_STREAM_NEW` | `[I32 kind, StrPtr options_json]` | `Handle` | `createDeflate`/`createInflate`/`createDeflateRaw`/`createInflateRaw`/`createGzip`/`createGunzip`/`createUnzip`/`createBrotliCompress`/`createBrotliDecompress`/`createZstdCompress`/`createZstdDecompress` |
| `__RTS_FN_NODE_ZLIB_STREAM_WRITE_ASYNC` | `[Handle stream, Handle inputBuf, I64 offset, I64 length, I32 flushKind, Handle callbackFn]` | `Void` | `.write(chunk)` on any stream instance — schedules the codec step on the threadpool, invokes `callbackFn` with `(err, Handle outputChunkOrNull)` per produced output chunk (possibly zero, one, or several times per write, matching Node's own chunked `'data'` emission) |
| `__RTS_FN_NODE_ZLIB_STREAM_FLUSH` | `[Handle stream, I32 flushKind, Handle callbackFn]` | `Void` | `zlibBase.flush([kind, ]callback)` |
| `__RTS_FN_NODE_ZLIB_STREAM_PARAMS` | `[Handle stream, I32 level, I32 strategy, Handle callbackFn]` | `Void` | `zlibBase.params(level, strategy, callback)` (Deflate/DeflateRaw only; native returns an error via the callback for unsupported stream kinds) |
| `__RTS_FN_NODE_ZLIB_STREAM_RESET` | `[Handle stream]` | `Void` | `zlibBase.reset()` (Inflate/Deflate zlib-based only; no-op/error surfaced per class support) |
| `__RTS_FN_NODE_ZLIB_STREAM_CLOSE` | `[Handle stream, Handle callbackFn]` | `Void` | `zlibBase.close(callback?)` |
| `__RTS_FN_NODE_ZLIB_STREAM_BYTES_WRITTEN` | `[Handle stream]` | `U64` | `zlibBase.bytesWritten` |
| `__RTS_FN_NODE_ZLIB_ONESHOT_SYNC` | `[I32 kind, Handle inputBuf, I64 offset, I64 length, StrPtr options_json]` | `Handle` (output Buffer) | `*Sync` convenience functions — blocking, whole-buffer codec call; errors surfaced through the engine's thread-local error slot (thrown by the `.ts` shim) |
| `__RTS_FN_NODE_ZLIB_ONESHOT_ASYNC` | `[I32 kind, Handle inputBuf, I64 offset, I64 length, StrPtr options_json, Handle callbackFn]` | `Void` | non-`Sync` convenience functions — `spawn_blocking` the whole-buffer codec call, then invoke `callbackFn` with `(err, Handle result)` |
| `__RTS_FN_NODE_ZLIB_CRC32` | `[Handle inputBuf, I64 offset, I64 length, U64 seed]` | `U64` | `zlib.crc32(data, value?)` |

Option objects (`level`/`memLevel`/`strategy`/`windowBits`/`chunkSize`/
`flush`/`finishFlush`/`maxOutputLength`/`info` plus the Brotli/Zstd `params`
maps and the `dictionary`/`pledgedSrcSize` fields) cross the ABI as one
JSON-encoded `StrPtr` (`options_json`) rather than dozens of positional
scalar parameters — same convention `node:crypto` uses for its
non-scalar-friendly option shapes (§5.2 of `crypto.md`). The `dictionary`
buffer, when present, is embedded in that JSON as a base64 string (small,
infrequently used, not worth a dedicated buffer-pointer parameter).

**`.ts` shim vs native extern split:**
- **Native externs**: every row above — stream lifecycle (`_NEW`/`_CLOSE`),
  incremental codec stepping (`_WRITE_ASYNC`/`_FLUSH`/`_PARAMS`/`_RESET`),
  one-shot buffer codec calls (`_ONESHOT_SYNC`/`_ONESHOT_ASYNC`), `bytesWritten`,
  and `crc32`.
- **`.ts` shim** (ships inside `rts-node`'s own bundled stdlib, not
  `rts-shared`): the `Deflate`/`Inflate`/`DeflateRaw`/`InflateRaw`/`Gzip`/
  `Gunzip`/`Unzip`/`BrotliCompress`/`BrotliDecompress`/`ZstdCompress`/
  `ZstdDecompress` classes as thin `stream.Transform` subclasses wiring
  `_transform`/`_flush` to the native write/flush externs; the 11
  `createX()` factory functions; the 22 convenience functions' default-arg
  handling and `kind` selection; JSON-stringifying the options object before
  it crosses the ABI; the `info: true` result-shape wrapping
  (`{ buffer, engine }`); and the entire `zlib.constants` object (a plain
  frozen `.ts` object literal of the numeric constant values — these are
  fixed integers from the underlying codec headers, not something that
  needs a native round-trip per lookup).

### 5.3 Async model

| Area | Sync | Callback | Promise |
|---|---|---|---|
| All 11 convenience compress/decompress functions (`deflate`/`inflate`/.../`zstdDecompress`) | ✅ `*Sync` variant — direct blocking native call, no threadpool | ✅ non-`Sync` variant → `spawn_blocking` + callback bridge | — (no native Promise form; commonly wrapped with `util.promisify` by user code, see §4/§7) |
| `crc32` | ✅ always sync, no callback/offload form exists in Node either | — | — |
| Streaming classes' `.write()`/`.end()` | — (there is no synchronous write path; every chunk goes through the codec asynchronously, matching Node's own zlib streams, which always offload to the threadpool even for a 1-byte write) | ✅ delivered via the stream's own `'data'`/`'end'`/`callback`-in-`_transform` machinery, not a user-facing Node-style `(err, result)` callback directly | — (streams are not Promise-shaped; `stream.pipeline`/`stream/promises` compose them with Promises at the `node:stream` layer, outside this module's scope) |
| `.flush()`/`.params()`/`.close()` | — | ✅ optional completion callback | — |
| `.reset()`/`.bytesWritten` | ✅ always sync (no threadpool involvement — cheap state reset / plain counter read) | — | — |

**Offload mechanism.** Every "→ `spawn_blocking`" cell uses the same pattern
as the rest of RTS's async surface (`docs/specs/async-promise-function.md`):
the callback-style call schedules the blocking codec computation on the
shared tokio runtime via `spawn_blocking`, then invokes the JS callback (a
`Function` handle) with `(err, result)` on completion, posted back through
the event loop — a plain Node-style callback, not a `promise.create`-wrapped
Promise (this module's own JS-visible surface never returns a native
Promise; see §5.7 for why the underlying async plumbing is still a
shared-infra dependency).

### 5.4 Multithread / worker interaction

- **Stream/codec handles are thread-owned, not shared.** A `Deflate`/
  `Gzip`/`Brotli*`/`Zstd*` instance's internal codec state (window buffer,
  in-flight partial block) must never be silently accessed from another RTS
  thread — this matches Node itself, where a live zlib stream is a plain
  `EventEmitter`-backed object with internal native state and is **not**
  structured-clonable across `worker_threads` (attempting to `postMessage` a
  `Gzip` instance throws `DataCloneError`). Under the RTS threading model
  (`docs/specs/rts-threading-model.md`), these handles live in a
  **per-thread region**; a handle-table lookup from another thread must fail
  fast (owning-thread-id tag check), not silently corrupt shared state.
- **Completed output buffers ARE shareable.** The `Buffer` a compression
  call produces is a plain `ArrayBuffer`-backed value with no special
  zlib-side state once returned — it follows the ordinary **shared-heap
  promotion on publication** story: a compressed/decompressed `Buffer`
  crossing a `channel`/`postMessage` boundary to another worker is promoted
  to the shared heap like any other `ArrayBuffer`/`SharedArrayBuffer`
  transfer, with no zlib-specific logic needed.
- **No process-wide singleton state.** Unlike `node:crypto`'s FIPS mode or
  secure-heap allocator, `node:zlib` has no genuinely global mutable state —
  `zlib.constants` is immutable data, and every codec instance is
  independent. Nothing here needs a cross-thread singleton.
- **Threadpool contention is the only cross-thread concern**, and it is
  already handled generically by the shared tokio runtime's worker pool
  sizing (`docs/specs/rts-threading-model.md` / `02-runtime.md`) — `node:zlib`
  does not need its own dedicated pool or scheduling policy, it is simply
  another category of `spawn_blocking` workload competing for the same
  runtime, same as `node:crypto`'s KDF/keygen offload work.

### 5.5 Buffer / TypedArray interop

- Every convenience function's `InputType` parameter (`Buffer | TypedArray |
  DataView | ArrayBuffer | string`) is normalized by the `.ts` shim to a raw
  byte span before crossing the ABI: a `string` is UTF-8-encoded first
  (matching Node's fixed, non-configurable input encoding for this module —
  there is no `inputEncoding` parameter, unlike `node:crypto`'s `BinaryLike`);
  everything else already has an underlying `ArrayBuffer` the native call
  reads directly via `arraybuffer_data_ptr`/`arraybuffer_byte_len` (no
  StrPtr copy, per §5.2).
- **Output allocation.** Because `flate2`/`brotli`'s one-shot APIs return an
  owned `Vec<u8>` of the exact final length once codec work completes, the
  native layer allocates a single right-sized `ArrayBuffer`/`Handle` for the
  result (no over-allocate-then-truncate step needed) and wraps it as a
  `Buffer` in the `.ts` shim, matching Node's own `Buffer`-returning
  convenience-function signatures.
- **Streaming output chunking.** For the incremental streaming classes,
  output is produced in `chunkSize`-ish increments (default 16 KiB) as the
  underlying codec's internal output buffer fills — each increment becomes
  one native-allocated `Handle`/`Buffer` delivered to the `.ts` `Transform`'s
  `push()`, exactly mirroring Node's own chunked `'data'` event cadence (a
  single large `.write()` can still produce many `'data'` events).
- **`dictionary` (zlib-based Deflate/Inflate family + Zstd)** and Brotli's
  lack of a dictionary concept: the dictionary buffer is small and
  infrequent enough that it is embedded as base64 inside the `options_json`
  string parameter (§5.2) rather than given its own ABI buffer-pointer slot —
  a deliberate simplicity-over-micro-optimization tradeoff, since dictionary
  use is rare relative to the hot compress/decompress data path.

### 5.6 Doctrine placement

`node:zlib` is unambiguously **non-primordial**: there is no native JS/TS
literal syntax for a compressed stream or a codec instance (contrast
`RegExp`'s `/re/` or `Error`'s `throw`/`catch` integration, or `Buffer`'s
reuse of the primordial `Uint8Array`/`ArrayBuffer` memory model). The engine
(`crates/rts-codegen-new/`) must never hardcode `"zlib"`, `"Gzip"`,
`"Deflate"`, `"BrotliCompress"`, or any other name from this module — it
only ever sees a fully-qualified member name like `node_zlib.createGzip` or
`node_zlib.gzipSync`, resolved through the existing node-registry data path
already wired in `crates/rts-node/src/lib.rs`:

```
import { gzipSync, createGzip } from "node:zlib"
        │
        ▼ ns_prefix_for("node:zlib") -> "node_zlib"        (data lookup, NODE_SPECS)
        │
        ▼ node_lookup("node_zlib.gzipSync")   -> &NodespaceMember { symbol: "__RTS_FN_NODE_ZLIB_ONESHOT_SYNC", ... }
        ▼ node_lookup("node_zlib.createGzip") -> &NodespaceMember { symbol: "__RTS_FN_NODE_ZLIB_STREAM_NEW", ... }
```

This is exactly the same generic mechanism the current `fs`/`path`/`os`/
`process`/`util`/`crypto` modules already use — adding `zlib::SPEC` to
`NODE_SPECS` in `lib.rs` is the entire "registration" surface the engine
needs; no codegen change is required to add this module, by construction of
the doctrine. Note that although several `NodespaceMember` rows for this
module share **one** underlying native symbol (`STREAM_NEW`/`ONESHOT_SYNC`/
`ONESHOT_ASYNC`, discriminated by the `kind` data value baked into that
member's row at table-construction time — see §5.2), each remains a distinct
data-table entry with its own `name`; the engine still resolves one name to
one row, it never branches on the module or class name itself.

The native-extern / `.ts`-shim split is as described in §5.2/§5.3: raw
codec/buffer operations + handle lifecycle are native `extern "C"` symbols
harvested into `NodespaceMember` rows; every JS-shaped class (the 11
`ZlibBase` subclasses), every factory function, every default-argument/
options-JSON-encoding concern, and the `zlib.constants` object are `.ts`
shipped inside `rts-node`'s own bundled stdlib (not `rts-shared` — only
*primordial* `.ts` lives in `rts-primitives`, and this module is not
primordial).

### 5.7 Shared-infra dependencies (FLAG)

- **Tokio runtime / `spawn_blocking`** (`async_rt.rs`'s shared multi-thread
  `OnceLock<Runtime>`) — needed for every non-`Sync` convenience function and
  for every streaming `.write()`/`.flush()`/`.params()` call (§5.3), since
  Node itself offloads all of these to its threadpool. **Currently lives in
  `rts-std`** (`crates/rts-runtime/.../runtime/async_rt.rs`). Since
  `rts-node` must not depend on `rts-std`, this needs to be hoisted into a
  shared low crate both can reach (as already flagged in `crypto.md` §5.7 —
  same underlying gap, not module-specific to zlib; whichever module lands
  the hoist first unblocks all subsequent ones).
- **Event loop / callback delivery** — the `(err, result)` callback for
  offloaded convenience calls and the per-chunk `(err, outputChunk)`
  callback for streaming writes must be posted back through the same event
  loop user code's `setTimeout`/microtask-driven callbacks run on, so
  ordering relative to other pending callbacks is correct. Currently
  `rts-std`-owned infrastructure. Same hoisting requirement as above.
- **Callback-invocation bridge** (`Function`'s `invoke_n` trampoline, used to
  call back into a JS callback with `(err, result)` from the offloaded
  thread) — needed for every callback-shaped path in §5.3. `Function` is
  **primordial**, so per the crate-partition doctrine its implementation
  belongs in `rts-primitives`, not `rts-std`; if in practice it still
  physically lives under `rts-std`'s `globals/function/ops.rs` at
  implementation time, that is a **pre-existing doctrine violation to fix
  first** (same note as `crypto.md` §5.7), not something `node:zlib` should
  work around with a duplicate trampoline.
- **Promise subsystem — NOT needed.** Unlike `node:crypto`'s `SubtleCrypto`,
  no part of `node:zlib`'s own JS-visible surface returns a native Promise
  (§5.3); `promise.create`/settle is not a dependency of this module. (User
  code layering `util.promisify` on top is a `node:util` concern, outside
  this module.)
- **TLS/rustls, net sockets, crypto primitives — not needed.** This module
  is pure in-memory codec transformation; it opens no socket, reads no
  filesystem, and needs no cryptographic primitive.
- **`node:stream`'s `Transform`/`Duplex` base machinery** — every streaming
  class (`Gzip`, `Inflate`, `BrotliCompress`, ...) is specified as *extending*
  `stream.Transform`. This is not "shared low-level infra" in the
  tokio/event-loop sense, but it **is** a hard cross-module prerequisite:
  if `node:stream`'s `Transform`/`Duplex`/backpressure plumbing is not yet
  implemented in `rts-node` when this module is built, the streaming half of
  this spec (§5.8h/i) cannot land as specified. Tracked as an open
  dependency in §7, not a "hoist from rts-std" item (stream is itself a
  `rts-node`-owned module, not rts-std infra).

If the tokio/event-loop hoist has not landed before this module is
implemented, the pragmatic fallback (matching `crypto.md`'s precedent) is:
ship the **`*Sync`** convenience surface first (§5.8b–d, g), which needs zero
async infra, and gate every callback/streaming path on the hoist landing.

### 5.8 Implementation phases

a. **Handle-table skeleton.** Reuse/extend `rts-node`'s private sharded
   handle table (established by whichever handle-bearing module lands
   first — see `crypto.md` §5.8a) plus `zlib::SPEC` registration in
   `lib.rs`, wired to zero real members yet.
b. **`crc32`.** Trivial, sync, no streaming state, immediately useful,
   wraps `crc32fast`.
c. **Raw deflate one-shot sync round trip.** `deflateRawSync`/
   `inflateRawSync` via `flate2` — simplest format (no header/trailer
   framing), smallest viable compress/decompress round trip.
d. **Zlib-wrapped deflate + gzip one-shot sync.** `deflateSync`/
   `inflateSync` (adds the zlib header/Adler32 trailer), then `gzipSync`/
   `gunzipSync` (adds gzip header/CRC32/size trailer) via `flate2`'s
   `Zlib*`/`Gz*` encoders/decoders.
e. **`unzipSync` auto-detection**, built on (c)/(d)'s codecs plus the
   magic-byte sniff described in §5.1.
f. **Async callback variants** of everything in (c)–(e) — `spawn_blocking` +
   `Function` callback bridge — for `deflate`/`inflate`/`deflateRaw`/
   `inflateRaw`/`gzip`/`gunzip`/`unzip` (gated on §5.7's tokio hoist landing,
   or implemented directly against `rts-std`'s current location as an
   explicit, temporary, justified exception if the hoist has not landed —
   state that tradeoff explicitly per the project's regression-discipline
   rule).
g. **Brotli one-shot sync + async.** `brotliCompressSync`/
   `brotliDecompressSync` via the `brotli` crate, `params` map → `quality`/
   `lgwin`/`mode` translation, then the callback-offloaded variants.
h. **Streaming classes, minimal viable Transform.** `createDeflate`/
   `createInflate`/`createGzip`/`createGunzip`/`createDeflateRaw`/
   `createInflateRaw`/`createUnzip`/`createBrotliCompress`/
   `createBrotliDecompress` backed by `flate2`'s/`brotli`'s incremental
   (`Compress`/`Decompress`/`CompressorWriter`) structs, wired to whatever
   `node:stream` `Transform` base exists at implementation time (or a
   zlib-local minimal Transform-alike shipped as an explicit interim if
   `node:stream` is not ready — see §7 tradeoff).
i. **Stream instance methods.** `.flush()`/`.params()`/`.reset()`/`.close()`/
   `.bytesWritten` wired to the `STREAM_FLUSH`/`STREAM_PARAMS`/`STREAM_RESET`/
   `STREAM_CLOSE`/`STREAM_BYTES_WRITTEN` externs.
j. **Zstd (experimental), lowest priority.** `createZstdCompress`/
   `createZstdDecompress` + `zstdCompress(Sync)`/`zstdDecompress(Sync)`, once
   the pure-Rust-vs-C-binding crate decision (§7) is made; matches Node's own
   Stability-1 classification for this sub-surface.

## 6. Test plan

- **Round-trip: raw deflate.** `inflateRawSync(deflateRawSync(input))` equals
  `input` for an empty buffer, a short ASCII string, and a multi-KB random
  byte buffer.
- **Round-trip: zlib deflate.** Same three inputs through
  `deflateSync`/`inflateSync`.
- **Round-trip: gzip.** Same three inputs through `gzipSync`/`gunzipSync`;
  additionally assert the output begins with the gzip magic bytes `1f 8b`.
- **Round-trip: Brotli.** Same three inputs through
  `brotliCompressSync`/`brotliDecompressSync`.
- **`unzipSync` auto-detection.** Feed it a `gzipSync(input)` output and
  separately a `deflateSync(input)` output; both must decompress back to
  `input` through the same `unzipSync` call without the caller specifying
  which framing was used.
- **Sync/callback/async equivalence.** For a fixed input + options, assert
  `gzipSync(input, opts)` equals the `Buffer` delivered by
  `gzip(input, opts, callback)` (byte-for-byte).
- **Compression level variation.** `deflateSync(largeText, { level:
  Z_BEST_SPEED })` vs `{ level: Z_BEST_COMPRESSION }` — assert both round-trip
  correctly and that `Z_BEST_COMPRESSION`'s output is not larger than
  `Z_BEST_SPEED`'s for a sufficiently compressible input.
- **`dictionary` option round trip.** `deflateSync(input, { dictionary })`
  followed by `inflateSync(output, { dictionary })` recovers `input`;
  omitting `dictionary` on the inflate side surfaces an error
  (`Z_NEED_DICT`/`Z_DATA_ERROR`-class).
- **Truncated-input handling.** `inflateSync` on a deliberately truncated
  `deflateSync` output throws by default; passing `{ finishFlush:
  Z_SYNC_FLUSH }` instead returns the partially-recovered data without
  throwing.
- **Gunzip trailing garbage.** Appending extra bytes after a valid gzip
  member and running it through `gunzip`/`createGunzip` surfaces an
  `'error'`.
- **`maxOutputLength` enforcement.** A small `maxOutputLength` on a call
  whose true decompressed size exceeds it surfaces an error rather than
  silently truncating.
- **Streaming pipe round trip.** Pipe a Readable of a multi-chunk payload
  through `createGzip()` into `createGunzip()` into a collecting Writable;
  assert the collected bytes equal the original payload, and that more than
  one `'data'` event fired on each stream for a sufficiently large payload
  (exercises chunking, not just a single-shot pass-through).
- **`.flush()` mid-stream.** Write a partial chunk to a `createGzip()`
  stream, call `.flush()`, and assert a `'data'` event fires before `.end()`
  is called (the flushed bytes are independently decompressible with
  `finishFlush: Z_SYNC_FLUSH` on the receiving side).
- **`.params()` mid-stream** on a `createDeflate()` instance — change level/
  strategy partway through and assert the stream still produces a valid,
  fully-round-trippable output when concatenated.
- **`.reset()`** on a `createDeflate()`/`createInflate()` instance between
  two independent, unrelated payloads — assert no cross-contamination
  between the pre- and post-reset compressed streams.
- **`crc32` correctness + chaining.** `crc32(Buffer.from(''))` equals the
  known empty-input CRC32 (`0`); `crc32(fullBuffer)` equals
  `crc32(secondHalf, crc32(firstHalf))` for a buffer split at an arbitrary
  midpoint (validates the seeded/chained form).
- **`zlib.constants` immutability.** Assert `zlib.constants.Z_OK === 0` (or
  whatever the platform's underlying value is) and that attempting to
  mutate a property on `zlib.constants` has no effect (frozen object).
- **Multithread: independent per-worker compression.** Spawn N
  `worker_threads` workers, each independently `gzipSync`-compressing a
  distinct payload derived from its worker index, `postMessage`-ing the
  compressed `Buffer` back to the main thread; assert every worker's result
  independently round-trips via `gunzipSync` on the main thread with no
  cross-worker data corruption (exercises §5.4's per-thread handle ownership
  + shared-heap-on-publication for the returned buffers).
- **Bad-input error shape.** `deflateSync(42)` (non-buffer/string input)
  throws a `TypeError`-class error; `crc32(42)` likewise.

## 7. Open questions / deferrals

- **Zstd crate choice.** The `zstd` crate's C-library binding
  (`zstd-sys`, requires a C compiler at build time) is the pragmatic first
  cut for `ZstdCompress`/`ZstdDecompress`, but is in tension with the
  project's general pure-Rust preference (mirrored elsewhere, e.g. TLS via
  `rustls` instead of OpenSSL). A pure-Rust decoder (`ruzstd`) exists;
  a mature pure-Rust *encoder* does not as of this writing. Since Node
  itself marks this whole sub-surface Experimental (Stability 1), resolving
  this is explicitly deferred to §5.8j and should not block the rest of the
  module.
- **`Unzip` auto-detection edge cases.** The hand-rolled magic-byte sniff
  (§5.1) is functionally equivalent to zlib's native `windowBits + 32` trick
  for well-formed gzip/zlib headers, but has not been checked against every
  malformed-header edge case zlib's own C implementation may special-case
  (e.g. a zlib stream with the `FDICT` bit set, or byte-for-byte-ambiguous
  short inputs). Needs a differential test against a real Node/zlib build
  before being considered parity-complete.
- **`BROTLI_OPERATION_EMIT_METADATA`.** Node's own docs describe this Brotli
  flush mode as impractical to use through the stream-based API "except by
  using the raw Brotli library directly" — no plan to expose it through
  RTS's streaming classes either; the constant is still listed (§2.3) for
  completeness since user code may reference it, but no native codepath
  will act on it distinctly from `BROTLI_OPERATION_PROCESS` until a concrete
  use case appears.
- **Exact `maxOutputLength`-exceeded error code/message text** and other
  zlib-specific errno-mapped strings (`ERR_BUFFER_TOO_LARGE`? a
  zlib-specific code?) need verification against a live Node 25 build —
  marked `(verify)`, not confirmed from the fetched docs.
- **`util.promisify` interplay** — depends on `node:util`'s promisify
  implementation recognizing the standard `(err, result)` callback shape (no
  special `util.promisify.custom` symbol is documented for zlib functions,
  so generic promisify wrapping should just work); noted as a cross-module
  coupling, not a blocker for this spec.
- **`node:stream` `Transform`/`Duplex` readiness** (§5.7) — the streaming
  half of this module (§5.8h/i) assumes a working `Transform` base class
  with correct backpressure semantics exists in `rts-node`. If it lands
  after this module is started, ship the one-shot/sync/callback surface
  first and gate streaming on it explicitly, or build a zlib-local minimal
  Transform-alike as a stated interim (to be deleted once the real one
  exists) — either is acceptable per the project's "shift focus to resolve
  a blocker first" rule, but the choice must be stated explicitly in the
  implementing commit/PR.
- **`ZstdOptions.pledgedSrcSize`** — only briefly surfaced in the fetched
  reference material as a compression-side size hint; its exact effect and
  interaction with streaming (vs one-shot) usage should be re-verified
  against the Node source/tests at implementation time.
