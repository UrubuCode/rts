# node:buffer

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:buffer` (Buffer is also an ambient global; Blob/File are also ambient globals since Node 18/20) |
| Node.js version | 25.x (`https://nodejs.org/docs/latest-v25.x/api/buffer.html`) |
| Stability | 2 - Stable |
| Tier | P0 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import { Buffer, Blob, File, atob, btoa, isAscii, isUtf8, transcode, resolveObjectURL, INSPECT_MAX_BYTES, kMaxLength, kStringMaxLength, constants } from 'node:buffer'`; CommonJS `const buffer = require('node:buffer')`; ambient globals `Buffer`, `Blob`, `File`, `atob`, `btoa` usable with **no import** in any module |
| Globals exposed | `Buffer` (class, always global), `Blob` (class, global since v18.0.0), `File` (class, global since v20.0.0), `atob`/`btoa` (functions, global since v16.0.0, legacy encodings) |

---

## 1. Purpose

`node:buffer` provides `Buffer`, Node's original API for working with raw binary
data, implemented as a subclass of the JS-standard `Uint8Array`. It also hosts
the WHATWG `Blob` and `File` classes (immutable, chunked byte containers with a
MIME type), the legacy `atob`/`btoa` base64 helpers, and a handful of top-level
utility functions (`transcode`, `isAscii`, `isUtf8`, `resolveObjectURL`) plus
size-limit constants. Because `Buffer`, `Blob`, `File`, `atob`, and `btoa` are
all injected as ambient globals by the real Node.js runtime, RTS must reproduce
both the `node:buffer` **module surface** and the **global-injection** behavior
without special-casing any of these names in the engine front-end.

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `Buffer` — extends `Uint8Array`

Not constructible directly in modern code (`new Buffer()` is deprecated, see
§4); the supported construction path is exclusively the static factory methods
below. All `Buffer` operations are **synchronous**.

**Static methods**

| Signature | Params | Returns | Throws | Variant |
|---|---|---|---|---|
| `Buffer.alloc(size, fill?, encoding?)` | `size: number`; `fill?: string \| Buffer \| Uint8Array \| number` (default `0`); `encoding?: BufferEncoding` (default `'utf8'`, only used when `fill` is a string) | `Buffer` | `ERR_INVALID_ARG_TYPE` (size not a number, ≥ v20); `ERR_OUT_OF_RANGE` (size < 0 or > `buffer.constants.MAX_LENGTH`); generic exception on invalid string `fill`/encoding; exception when filling a non-zero-length buffer with a zero-length `fill` | sync |
| `Buffer.allocUnsafe(size)` | `size: number` | `Buffer` (uninitialized memory) | `ERR_INVALID_ARG_TYPE`; `ERR_OUT_OF_RANGE` | sync |
| `Buffer.allocUnsafeSlow(size)` | `size: number` | `Buffer` (uninitialized, never pooled) | `ERR_INVALID_ARG_TYPE`; `ERR_OUT_OF_RANGE` | sync |
| `Buffer.from(array)` | `array: number[] \| Iterable<number>` | `Buffer` | `TypeError` on non-array-like | sync |
| `Buffer.from(arrayBuffer, byteOffset?, length?)` | `arrayBuffer: ArrayBuffer \| SharedArrayBuffer`; `byteOffset?: number` (default `0`); `length?: number` (default `arrayBuffer.byteLength - byteOffset`) | `Buffer` (view, **shares memory**) | `ERR_OUT_OF_RANGE` (offset/length out of bounds) | sync |
| `Buffer.from(buffer)` | `buffer: Buffer \| Uint8Array` | `Buffer` (copy) | `TypeError` | sync |
| `Buffer.from(object, offsetOrEncoding?, length?)` | `object: { valueOf(): unknown } \| { [Symbol.toPrimitive](hint: string): unknown } \| ArrayLike<number>`; `offsetOrEncoding?: number \| string`; `length?: number` | `Buffer` | `TypeError` (object has neither shape) | sync |
| `Buffer.from(string, encoding?)` | `string: string`; `encoding?: BufferEncoding` (default `'utf8'`) | `Buffer` | `TypeError` (not a string); generic exception on unknown encoding | sync |
| `Buffer.isBuffer(obj)` | `obj: unknown` | `boolean` | — | sync |
| `Buffer.isEncoding(encoding)` | `encoding: string` | `boolean` | — | sync |
| `Buffer.byteLength(string, encoding?)` | `string: string \| Buffer \| TypedArray \| DataView \| ArrayBuffer \| SharedArrayBuffer`; `encoding?: BufferEncoding` (default `'utf8'`, ignored for non-string input) | `number` | — (may over-report for non-base64/hex-valid input to those encodings) | sync |
| `Buffer.compare(buf1, buf2)` | `buf1: Buffer \| Uint8Array`; `buf2: Buffer \| Uint8Array` | `-1 \| 0 \| 1` | `TypeError` (not Buffer/Uint8Array) | sync |
| `Buffer.concat(list, totalLength?)` | `list: (Buffer \| Uint8Array)[]`; `totalLength?: number` | `Buffer` | `TypeError` (list not an array) | sync |
| `Buffer.copyBytesFrom(view, offset?, length?)` | `view: TypedArray`; `offset?: number` (default `0`, in elements); `length?: number` (default `view.length - offset`, in elements) | `Buffer` (byte-for-byte copy, v19.8.0+/v18.16.0+) | `ERR_OUT_OF_RANGE`; `ERR_INVALID_ARG_TYPE` (view not a TypedArray) | sync |

**Static properties**

| Property | Type | Notes |
|---|---|---|
| `Buffer.poolSize` | `number` | Default `8192`. Size of the internal pre-allocated pool `allocUnsafe`/`from(string)`/`from(array)`/`concat` draw from. Mutable. |
| `Buffer.prototype` | `object` | Standard prototype object. |

**Instance properties**

| Property | Type | Notes |
|---|---|---|
| `buf[index]` | `number` (0-255) | Indexed byte accessor, inherited from `Uint8Array`. |
| `buf.buffer` | `ArrayBuffer` | Underlying `ArrayBuffer` (inherited). |
| `buf.byteOffset` | `number` | Offset into `buf.buffer` (inherited). |
| `buf.length` | `number` | Number of bytes (inherited as `Uint8Array.prototype.length`). |
| `buf.parent` | `Buffer \| undefined` | **Deprecated**, use `buf.buffer` instead. |

**Instance methods**

| Signature | Params | Returns | Throws | Variant |
|---|---|---|---|---|
| `buf.compare(target, targetStart?, targetEnd?, sourceStart?, sourceEnd?)` | `target: Buffer \| Uint8Array`; `targetStart?: number` (default `0`); `targetEnd?: number` (default `target.length`); `sourceStart?: number` (default `0`); `sourceEnd?: number` (default `buf.length`) | `-1 \| 0 \| 1` | `ERR_OUT_OF_RANGE` (any bound out of range) | sync |
| `buf.copy(target, targetStart?, sourceStart?, sourceEnd?)` | `target: Buffer \| Uint8Array`; `targetStart?: number` (default `0`); `sourceStart?: number` (default `0`); `sourceEnd?: number` (default `buf.length`) | `number` (bytes copied) | — (out-of-range args are clamped, not thrown, per spec) | sync |
| `buf.entries()` | — | `Iterator<[number, number]>` | — | sync |
| `buf.equals(otherBuffer)` | `otherBuffer: Buffer \| Uint8Array` | `boolean` | `TypeError` | sync |
| `buf.fill(value, offset?, end?, encoding?)` | `value: string \| Buffer \| Uint8Array \| number`; `offset?: number` (default `0`); `end?: number` (default `buf.length`); `encoding?: BufferEncoding` (default `'utf8'`) | `Buffer` (`this`) | `ERR_OUT_OF_RANGE`; exception on invalid string value / unknown encoding | sync |
| `buf.includes(value, byteOffset?, encoding?)` | `value: string \| Buffer \| Uint8Array \| number`; `byteOffset?: number` (default `0`); `encoding?: BufferEncoding` (default `'utf8'`) | `boolean` | — | sync |
| `buf.indexOf(value, byteOffset?, encoding?)` | same as `includes` | `number` (`-1` if not found) | — | sync |
| `buf.keys()` | — | `Iterator<number>` | — | sync |
| `buf.lastIndexOf(value, byteOffset?, encoding?)` | same as `includes` | `number` (`-1` if not found) | — | sync |
| `buf.slice(start?, end?)` | `start?: number` (default `0`); `end?: number` (default `buf.length`) | `Buffer` (**view**, not a copy) | — | sync |
| `buf.subarray(start?, end?)` | same as `slice` | `Buffer` (**view**, preferred over `slice`) | — | sync |
| `buf.swap16()` | — | `Buffer` (`this`) | `RangeError` (length not multiple of 2) | sync |
| `buf.swap32()` | — | `Buffer` (`this`) | `RangeError` (length not multiple of 4) | sync |
| `buf.swap64()` | — | `Buffer` (`this`) | `RangeError` (length not multiple of 8) | sync |
| `buf.toJSON()` | — | `{ type: 'Buffer', data: number[] }` | — | sync |
| `buf.toString(encoding?, start?, end?)` | `encoding?: BufferEncoding` (default `'utf8'`); `start?: number` (default `0`); `end?: number` (default `buf.length`) | `string` | exception on unknown encoding | sync |
| `buf.values()` | — | `Iterator<number>` (also `buf[Symbol.iterator]`) | — | sync |
| `buf.write(string, offset?, length?, encoding?)` | `string: string`; `offset?: number` (default `0`); `length?: number` (default `buf.length - offset`); `encoding?: BufferEncoding` (default `'utf8'`) | `number` (bytes written) | `ERR_OUT_OF_RANGE`; exception on unknown encoding | sync |
| `buf[util.inspect.custom]` / `buf.inspect()` | internal, used by `console.log`/`util.inspect` | `string` (hex dump, truncated to `buffer.INSPECT_MAX_BYTES`) | — | sync |

**Instance methods — numeric read** (all sync, all throw `ERR_OUT_OF_RANGE` /
`RangeError` when `offset`/`offset+size` exceeds `buf.length`, and
`ERR_INVALID_ARG_TYPE` when `offset` is not an integer):

| Method | `offset` param | `byteLength` param | Returns |
|---|---|---|---|
| `buf.readInt8(offset?)` | default `0` | — | `number` |
| `buf.readUInt8(offset?)` | default `0` | — | `number` |
| `buf.readInt16BE(offset?)` | default `0` | — | `number` |
| `buf.readInt16LE(offset?)` | default `0` | — | `number` |
| `buf.readUInt16BE(offset?)` | default `0` | — | `number` |
| `buf.readUInt16LE(offset?)` | default `0` | — | `number` |
| `buf.readInt32BE(offset?)` | default `0` | — | `number` |
| `buf.readInt32LE(offset?)` | default `0` | — | `number` |
| `buf.readUInt32BE(offset?)` | default `0` | — | `number` |
| `buf.readUInt32LE(offset?)` | default `0` | — | `number` |
| `buf.readIntBE(offset, byteLength)` | required | required, `1..6` | `number` |
| `buf.readIntLE(offset, byteLength)` | required | required, `1..6` | `number` |
| `buf.readUIntBE(offset, byteLength)` | required | required, `1..6` | `number` |
| `buf.readUIntLE(offset, byteLength)` | required | required, `1..6` | `number` |
| `buf.readBigInt64BE(offset?)` | default `0` | — | `bigint` |
| `buf.readBigInt64LE(offset?)` | default `0` | — | `bigint` |
| `buf.readBigUInt64BE(offset?)` | default `0` | — | `bigint` |
| `buf.readBigUInt64LE(offset?)` | default `0` | — | `bigint` |
| `buf.readFloatBE(offset?)` | default `0` | — | `number` |
| `buf.readFloatLE(offset?)` | default `0` | — | `number` |
| `buf.readDoubleBE(offset?)` | default `0` | — | `number` |
| `buf.readDoubleLE(offset?)` | default `0` | — | `number` |

**Instance methods — numeric write** (all sync, all throw `ERR_OUT_OF_RANGE` on
out-of-bounds `offset` and on `value` outside the representable range for the
target width):

| Method | `value` param | `offset` param | `byteLength` param | Returns |
|---|---|---|---|---|
| `buf.writeInt8(value, offset?)` | required | default `0` | — | `number` (offset + bytes written) |
| `buf.writeUInt8(value, offset?)` | required | default `0` | — | `number` |
| `buf.writeInt16BE(value, offset?)` | required | default `0` | — | `number` |
| `buf.writeInt16LE(value, offset?)` | required | default `0` | — | `number` |
| `buf.writeUInt16BE(value, offset?)` | required | default `0` | — | `number` |
| `buf.writeUInt16LE(value, offset?)` | required | default `0` | — | `number` |
| `buf.writeInt32BE(value, offset?)` | required | default `0` | — | `number` |
| `buf.writeInt32LE(value, offset?)` | required | default `0` | — | `number` |
| `buf.writeUInt32BE(value, offset?)` | required | default `0` | — | `number` |
| `buf.writeUInt32LE(value, offset?)` | required | default `0` | — | `number` |
| `buf.writeIntBE(value, offset, byteLength)` | required | required | required, `1..6` | `number` |
| `buf.writeIntLE(value, offset, byteLength)` | required | required | required, `1..6` | `number` |
| `buf.writeUIntBE(value, offset, byteLength)` | required | required | required, `1..6` | `number` |
| `buf.writeUIntLE(value, offset, byteLength)` | required | required | required, `1..6` | `number` |
| `buf.writeBigInt64BE(value, offset?)` | required (`bigint`) | default `0` | — | `number` |
| `buf.writeBigInt64LE(value, offset?)` | required (`bigint`) | default `0` | — | `number` |
| `buf.writeBigUInt64BE(value, offset?)` | required (`bigint`) | default `0` | — | `number` |
| `buf.writeBigUInt64LE(value, offset?)` | required (`bigint`) | default `0` | — | `number` |
| `buf.writeFloatBE(value, offset?)` | required | default `0` | — | `number` |
| `buf.writeFloatLE(value, offset?)` | required | default `0` | — | `number` |
| `buf.writeDoubleBE(value, offset?)` | required | default `0` | — | `number` |
| `buf.writeDoubleLE(value, offset?)` | required | default `0` | — | `number` |

**Deprecated constructors** (DEP0005 — still functional, must still be
supported for compat):

```typescript
new Buffer(array: number[]): Buffer
new Buffer(arrayBuffer: ArrayBuffer, byteOffset?: number, length?: number): Buffer
new Buffer(buffer: Buffer): Buffer
new Buffer(size: number): Buffer
new Buffer(string: string, encoding?: string): Buffer
```

**Events:** none. `Buffer` is not an `EventEmitter`.

---

#### `Blob` — no base class (implements the WHATWG `Blob` interface)

```typescript
new buffer.Blob(sources?: BlobPart[], options?: BlobOptions): Blob
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `sources` | `BlobPart[]` (`string \| ArrayBuffer \| TypedArray \| DataView \| Blob`) | yes | `[]` |
| `options` | `BlobOptions` | yes | `{}` |

**Instance properties**

| Property | Type | Notes |
|---|---|---|
| `blob.size` | `number` | Total size in bytes (read-only). |
| `blob.type` | `string` | MIME type, lowercased if set; empty string if unset (read-only, **not validated**). |

**Instance methods**

| Signature | Params | Returns | Throws | Variant |
|---|---|---|---|---|
| `blob.arrayBuffer()` | — | `Promise<ArrayBuffer>` | rejects on internal read failure | promise |
| `blob.bytes()` | — | `Promise<Uint8Array>` | rejects on internal read failure | promise |
| `blob.slice(start?, end?, type?)` | `start?: number` (default `0`); `end?: number` (default `blob.size`); `type?: string` (default `''`) | `Blob` (new, view over the same bytes) | — | sync |
| `blob.stream()` | — | `ReadableStream` | — | sync (returns a stream object; consuming it is async) |
| `blob.text()` | — | `Promise<string>` (UTF-8 decoded) | rejects on internal read failure | promise |

**Events:** none. `Blob` is not an `EventEmitter`.

---

#### `File` — extends `Blob`

```typescript
new buffer.File(sources: BlobPart[], fileName: string, options?: FileOptions): File
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `sources` | `BlobPart[]` | no | — |
| `fileName` | `string` | no | — |
| `options` | `FileOptions` | yes | `{}` |

**Instance properties** (in addition to inherited `size`/`type`)

| Property | Type | Notes |
|---|---|---|
| `file.name` | `string` | File name as given at construction. |
| `file.lastModified` | `number` | Last-modified timestamp in ms since epoch; from `options.lastModified` or `Date.now()` at construction. |

**Instance methods:** none of its own — inherits `arrayBuffer()`, `bytes()`,
`slice()`, `stream()`, `text()` from `Blob`.

**Events:** none.

---

### 2.2 Top-level functions (`node:buffer` module)

| Signature | Params | Returns | Throws | Variant |
|---|---|---|---|---|
| `buffer.atob(data)` | `data: string` (base64) | `string` (binary string, one code unit per byte) | throws on invalid base64 characters (`InvalidCharacterError`-shaped `DOMException`/`Error`, verify exact ctor RTS should emit) | sync |
| `buffer.btoa(data)` | `data: string` (binary string, chars must be in `U+0000`-`U+00FF`) | `string` (base64) | throws if any char code point > 255 (`InvalidCharacterError`-shaped) | sync |
| `buffer.isAscii(input)` | `input: Buffer \| Uint8Array \| ArrayBuffer` | `boolean` — `true` iff every byte is `<= 0x7F` | `ERR_INVALID_ARG_TYPE` | sync |
| `buffer.isUtf8(input)` | `input: Buffer \| Uint8Array \| ArrayBuffer` | `boolean` — `true` iff bytes are valid UTF-8 | `ERR_INVALID_ARG_TYPE` | sync |
| `buffer.transcode(source, fromEnc, toEnc)` | `source: Buffer \| Uint8Array`; `fromEnc: TranscodeEncoding`; `toEnc: TranscodeEncoding` (encodings restricted to `'ascii' \| 'utf8' \| 'utf16le' \| 'ucs2' \| 'latin1' \| 'binary'` — **not** base64/hex) | `Buffer` (re-encoded, lossy for unrepresentable code points → `?`) | `ERR_INVALID_ARG_TYPE` (source not Buffer/Uint8Array); `ERR_UNKNOWN_ENCODING` (unsupported `fromEnc`/`toEnc`) | sync |
| `buffer.resolveObjectURL(id)` | `id: string` (a `blob:nodedata:...` URL string previously returned by `URL.createObjectURL(blob)`) | `Blob \| undefined` | — | sync |

**Note:** `buffer.kMaxLength` and `buffer.constants.MAX_LENGTH` govern the size
ceiling checked by `Buffer.alloc`/`allocUnsafe`/`allocUnsafeSlow` (see §2.3).

### 2.3 Properties & constants

| Name | Type | Notes |
|---|---|---|
| `buffer.INSPECT_MAX_BYTES` | `number` | Default `50`. Bytes shown by `util.inspect`/`console.log` before truncating with `... N more bytes`. Mutable. |
| `buffer.kMaxLength` | `number` | Alias of `buffer.constants.MAX_LENGTH`. Largest allowed `Buffer` (bytes). Platform-dependent (32-bit vs 64-bit); mark exact value `(verify)` against the RTS host's pointer width at implementation time. |
| `buffer.kStringMaxLength` | `number` | Alias of `buffer.constants.MAX_STRING_LENGTH`. Largest allowed JS string length RTS's string representation can hold. `(verify)` exact ceiling. |
| `buffer.constants.MAX_LENGTH` | `number` | Same as `kMaxLength`. |
| `buffer.constants.MAX_STRING_LENGTH` | `number` | Same as `kStringMaxLength`. |
| `Buffer.poolSize` | `number` | See §2.1 static properties. |

### 2.4 Events

None. No class in `node:buffer` extends `EventEmitter`.

---

## 3. Types & option objects

```typescript
type BufferEncoding =
  | 'ascii'
  | 'utf8' | 'utf-8'
  | 'utf16le' | 'utf-16le'
  | 'ucs2' | 'ucs-2'
  | 'base64'
  | 'base64url'
  | 'latin1'
  | 'binary'
  | 'hex';

// Subset accepted by buffer.transcode() — no binary-to-text codecs.
type TranscodeEncoding =
  | 'ascii'
  | 'utf8'
  | 'utf16le'
  | 'ucs2'
  | 'latin1'
  | 'binary';

type EndingsMode = 'transparent' | 'native';

interface BlobOptions {
  /** Line-ending conversion for string sources. Default: 'transparent'. */
  endings?: EndingsMode;
  /** MIME type; not validated. Default: ''. */
  type?: string;
}

interface FileOptions extends BlobOptions {
  /** ms since epoch; default: Date.now() at construction time. */
  lastModified?: number;
}

type BlobPart = string | ArrayBuffer | ArrayBufferView | Blob;

// Buffer.from(object, ...) accepted object shapes:
interface ObjectWithValueOf {
  valueOf(): string | ArrayBuffer | Uint8Array | number[];
}
interface ObjectWithToPrimitive {
  [Symbol.toPrimitive](hint: 'string' | 'number' | 'default'): unknown;
}
interface ArrayLikeOfBytes {
  length: number;
  [index: number]: number;
}

// buf.toJSON() shape:
interface BufferJSON {
  type: 'Buffer';
  data: number[];
}

// Node error shape thrown by out-of-range/invalid-arg paths:
interface NodeSystemError extends Error {
  code: 'ERR_OUT_OF_RANGE' | 'ERR_INVALID_ARG_TYPE' | 'ERR_INVALID_ARG_VALUE' | 'ERR_UNKNOWN_ENCODING' | 'ERR_BUFFER_OUT_OF_BOUNDS';
}
```

---

## 4. Node semantics & edge cases

- **Error codes.** `ERR_OUT_OF_RANGE` (size/offset outside valid bounds, e.g.
  `Buffer.alloc(-1)`, `buf.readInt8(buf.length)`); `ERR_INVALID_ARG_TYPE`
  (`size` not a number since v20, wrong source type for `Buffer.from`,
  `transcode` source not Buffer/Uint8Array); `ERR_INVALID_ARG_VALUE` (pre-v20
  equivalent of the above two for alloc family — superseded); `ERR_UNKNOWN_ENCODING`
  (`transcode` with an unsupported encoding name; `Buffer.isEncoding` never
  throws — it returns `false` instead); `ERR_BUFFER_OUT_OF_BOUNDS` (verify —
  historically used internally for zero-arg bound violations on
  `fill`/`copy`). Plain `RangeError` is used for `swap16/32/64` on a
  non-multiple length and for numeric-write range violations.
- **Buffer size ceiling.** `buffer.constants.MAX_LENGTH` differs by pointer
  width and V8 build; treat as a runtime-queried constant, not a compile-time
  literal, in the RTS implementation.
- **Hex decoding is best-effort and truncates:** `Buffer.from('1ag123', 'hex')`
  stops at the first invalid hex character (`'g'`) → `<1a>`; an odd trailing
  digit is dropped (`'1a7'` → `<1a>`).
- **Base64/base64url are permissive on decode:** whitespace (space/tab/
  newline) inside a base64 string is ignored; `'base64'` decoding also accepts
  the URL-safe alphabet (RFC 4648 §5); `'base64url'` **encoding** omits `=`
  padding; decoding accepts standard base64 too.
- **`byteLength()` can over-report** for `'base64'`/`'base64url'`/`'hex'`
  strings containing characters invalid for those encodings (it assumes valid
  input for speed).
- **latin1/binary:** only `U+0000`–`U+00FF`; out-of-range code points are
  **silently truncated/mapped**, not rejected. `'binary'` is a pure alias of
  `'latin1'`.
- **utf16le/ucs2:** Node only ever supports the **little-endian** variant;
  there is no `'utf16be'`. Unlike historical UCS-2, Node always supports
  code points above `U+FFFF` (surrogate pairs), 2 or 4 bytes per character.
- **`Buffer` is a `Uint8Array` subclass**, but:
  - `buf.slice()` returns a **view** (no copy) — this differs from
    `TypedArray.prototype.slice()`, which copies. `buf.subarray()` is the
    view-returning method for both and should be preferred by new code.
  - `buf.toString()` is **not** the same as `TypedArray.prototype.toString()`.
  - Passing a `Buffer` to a plain `TypedArray` constructor
    (`new Uint16Array(buf)`) copies elements **as numeric values**, not as raw
    bytes — use `Buffer.copyBytesFrom()` for a true byte-for-byte reinterpret.
  - `Buffer.prototype` methods are generic enough to `.call()` on a plain
    `Uint8Array`.
- **Deprecations:** `new Buffer(...)` (DEP0005) — still works, must route to
  the same static-factory logic as `Buffer.from`/`Buffer.alloc` based on the
  first argument's type. `buf.parent` is a legacy alias, superseded by
  `buf.buffer`.
- **Pool internals:** `Buffer.allocUnsafe(n)` draws from a shared
  `Buffer.poolSize`-byte (default 8 KiB) pool when `n < poolSize >>> 1` (half
  the pool size, 4 KiB by default); `Buffer.alloc()` **never** uses the pool
  (it always zero-fills fresh memory, which is measurably slower);
  `Buffer.allocUnsafeSlow()` never uses the pool either (for long-lived small
  buffers that would otherwise pin a whole pool slab alive). `Buffer.from(array)`,
  `Buffer.from(string)`, and `Buffer.concat()` may also draw from the pool.
- **Security note:** `Buffer.allocUnsafe`/`allocUnsafeSlow` return
  **uninitialized memory** that may contain previously-freed sensitive data —
  callers must fully overwrite before exposing; `Buffer.alloc` zero-fills and
  is the safe default.
- **`JSON.stringify(buf)`** uses `buf.toJSON()` → `{ type: 'Buffer', data: [...] }`,
  not a base64/string form.
- **Blob is immutable after construction:** `ArrayBuffer`/`TypedArray`/
  `DataView`/`Buffer` sources are **copied** into the `Blob` at construction
  time, so the caller may safely mutate the source afterward. String sources
  are UTF-8 encoded; unmatched surrogates become `U+FFFD`.
- **Blob `endings: 'native'`** converts line endings to the host platform's
  `os.EOL` (`\r\n` on Windows, `\n` on POSIX) — a genuine Windows/POSIX
  behavioral difference to replicate.
- **Blob cross-thread sharing:** a `Blob` can be `postMessage`'d through a
  `MessagePort` to multiple destinations without an explicit transfer list and
  without copying its bytes eagerly — the underlying byte copy only happens
  when `arrayBuffer()`/`bytes()`/`text()` is actually called on the receiving
  side, and the original `Blob` remains usable after posting.
- **No backpressure/ordering concerns** for `Buffer`/`Blob`/`File` — every
  operation except the four Promise-returning `Blob`/`File` methods is
  synchronous over already-resident memory. `blob.stream()` returns a Web
  `ReadableStream`, which does have its own backpressure semantics, but those
  are out of scope for this module (owned by the streams/web surface).

---

## 5. RTS implementation notes

### 5.1 Native impl mapping

- **Byte storage is not reimplemented in rts-node.** `ArrayBuffer`/
  `TypedArray` are **primordial** (owned by the engine + `rts-primitives`,
  per the PRIMORDIAL-vs-REGISTRY doctrine) and `rts-engine::heap::handles`
  already has a stable-pointer `Entry::ArrayBuffer(Box<ArrayBufferData>)`
  variant (built for N-API: `alloc_arraybuffer_owned`, `arraybuffer_data_ptr`,
  `arraybuffer_byte_len`, `arraybuffer_detach`). `Buffer` is modeled as
  **exactly a `Uint8Array` instance** (same shape/handle representation) with
  extra prototype methods — rts-node adds no new byte-storage primitive, it
  reuses the engine's existing `ArrayBuffer` handle and the engine's
  already-lowered TypedArray element indexing.
- **String <-> bytes codecs are rts-node's own, independent implementation**
  (rts-node cannot depend on `rts-std`, so it cannot reuse
  `rts-std::crypto`'s existing base64/hex helpers — see §5.7):
  - `utf8`: `std::str::from_utf8`/`String::as_bytes` (native Rust, no crate).
  - `latin1`/`binary`/`ascii`: direct byte-for-byte mapping with truncation
    (no crate).
  - `utf16le`/`ucs2`: manual UTF-16 <-> UTF-8 transcode via
    `char::encode_utf16`/`char::decode_utf16` (std lib), with `U+FFFD`
    substitution for unpaired surrogates.
  - `hex`: hand-rolled encode/decode (trivial, no crate needed) implementing
    Node's truncate-on-invalid-nibble behavior exactly.
  - `base64`/`base64url`: vendor the `base64` crate (or hand-roll) as a
    **direct rts-node dependency** — a deliberate, accepted duplication of
    logic that also exists in `rts-std::crypto`, per the owner's
    full-independence decision for rts-node.
- **`Blob`/`File`** are modeled as a small Rust struct
  `{ bytes: Arc<[u8]>, mime: String, endings_applied: bool }` (`File` adds
  `name: String, last_modified_ms: i64`), stored behind a dedicated rts-node
  handle table (see §5.2) — **not** a new `rts-engine::heap::handles::Entry`
  variant, since Blob/File bytes are immutable-after-construction and never
  need GC-precise scanning as a mutable byte buffer the way `ArrayBuffer`
  does; a simple `Arc<[u8]>` reference-counted allocation is sufficient and
  cheap to share across `.slice()` views (a `Blob` slice is a
  `(Arc<[u8]> offset, len)` view, never a copy).
- **`atob`/`btoa`** reuse the same base64 codec as `Buffer`'s `'base64'`
  encoding; they operate directly on `StrPtr` in/out with no Buffer/Handle
  involved.
- **`transcode`** composes the decode-to-`utf8`-intermediate + re-encode
  codecs above; it never needs an external ICU/iconv dependency since the
  6 supported encodings are all natively covered.
- **`resolveObjectURL`** needs a lookup table keyed by the `blob:nodedata:...`
  id string; the table itself is populated by `URL.createObjectURL(blob)`,
  which is **not** part of `node:buffer` (it is a Web/URL global implemented
  elsewhere) — flagged as a cross-module coordination point in §5.7/§7.

### 5.2 ABI surface

Proposed symbol convention: `__RTS_FN_NODE_BUFFER_<NAME>`. `Buffer`/`Uint8Array`
values cross the ABI boundary as `Handle` (the 48-bit payload already is a
HandleTable slot per the `PolyValue` doctrine); raw `string` values cross as
`StrPtr` (`ptr:i64, len:i64`, UTF-8); `Blob`/`File` objects get their own
opaque `Handle` into an rts-node-owned table (distinct handle space from the
engine's `HandleTable`, exposed the same way: an rts-node
`OnceLock<Mutex<slab::Slab<BlobEntry>>>` per the `02-runtime.md` shared-state
pattern, or a shard-aware table modeled on the engine's if contention becomes
a concern).

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_BUFFER_ALLOC` | `I64 size, Handle fill_buf_or_void, StrPtr fill_str, Bool has_str_fill, StrPtr encoding` | `Handle` | zero-fills unless `fill` given |
| `__RTS_FN_NODE_BUFFER_ALLOC_UNSAFE` | `I64 size` | `Handle` | uninitialized, pool-eligible |
| `__RTS_FN_NODE_BUFFER_ALLOC_UNSAFE_SLOW` | `I64 size` | `Handle` | uninitialized, never pooled |
| `__RTS_FN_NODE_BUFFER_FROM_STRING` | `StrPtr s, StrPtr encoding` | `Handle` | encodes via §5.1 codec table |
| `__RTS_FN_NODE_BUFFER_FROM_BYTES` | `Handle src_view, I64 offset, I64 length` | `Handle` | copy path (`Buffer.from(buffer\|array)`) |
| `__RTS_FN_NODE_BUFFER_FROM_ARRAYBUFFER_VIEW` | `Handle array_buffer, I64 byte_offset, I64 length` | `Handle` | shares memory (calls `arraybuffer_data_ptr`) |
| `__RTS_FN_NODE_BUFFER_IS_BUFFER` | `Handle maybe_buf` | `Bool` | tag check against the Buffer shape marker |
| `__RTS_FN_NODE_BUFFER_IS_ENCODING` | `StrPtr encoding` | `Bool` | table lookup, never throws |
| `__RTS_FN_NODE_BUFFER_BYTE_LENGTH` | `StrPtr s, StrPtr encoding` | `U64` | string-source overload; a separate overload takes `Handle` for Buffer/TypedArray/ArrayBuffer sources |
| `__RTS_FN_NODE_BUFFER_COMPARE` | `Handle a, Handle b` | `I32` (-1/0/1) | |
| `__RTS_FN_NODE_BUFFER_CONCAT` | `Handle list_vec, I64 total_length_or_neg1` | `Handle` | `list_vec` is an engine-owned array of Buffer handles |
| `__RTS_FN_NODE_BUFFER_COPY_BYTES_FROM` | `Handle view, I64 offset, I64 length` | `Handle` | element-count params, byte-for-byte copy |
| `__RTS_FN_NODE_BUFFER_INSTANCE_COMPARE` / `_COPY` / `_EQUALS` / `_FILL` / `_INCLUDES` / `_INDEX_OF` / `_LAST_INDEX_OF` / `_SLICE` / `_SUBARRAY` / `_SWAP16` / `_SWAP32` / `_SWAP64` / `_TO_STRING` / `_WRITE` | `Handle self, ...` per-method params (`I64`/`StrPtr`/`Handle` as applicable) | per-method (`Handle`/`I32`/`Bool`/`StrPtr`/`Void`) | one symbol per instance method listed in §2.1 |
| `__RTS_FN_NODE_BUFFER_READ_<KIND>` (22 symbols, one per row in the §2.1 read table) | `Handle self, I64 offset[, I64 byte_length]` | `I64`/`U64`/`F64` (bigint variants return `I64`/`U64` reinterpreted as the JS BigInt primordial) | bounds-checked, traps map to `ERR_OUT_OF_RANGE` |
| `__RTS_FN_NODE_BUFFER_WRITE_<KIND>` (22 symbols, one per row in the §2.1 write table) | `Handle self, I64/F64/U64 value, I64 offset[, I64 byte_length]` | `I64` (new offset) | same bounds-check discipline |
| `__RTS_FN_NODE_BUFFER_TRANSCODE` | `Handle source, StrPtr from_enc, StrPtr to_enc` | `Handle` | composes decode+encode codecs |
| `__RTS_FN_NODE_BUFFER_ATOB` | `StrPtr data` | `StrPtr` | pure string in/out, no Handle |
| `__RTS_FN_NODE_BUFFER_BTOA` | `StrPtr data` | `StrPtr` | pure string in/out |
| `__RTS_FN_NODE_BUFFER_IS_ASCII` | `Handle input` | `Bool` | |
| `__RTS_FN_NODE_BUFFER_IS_UTF8` | `Handle input` | `Bool` | |
| `__RTS_FN_NODE_BLOB_NEW` | `Handle parts_vec, StrPtr endings, StrPtr mime_type` | `Handle` | allocates in the rts-node Blob table |
| `__RTS_FN_NODE_BLOB_SIZE` / `_TYPE` | `Handle self` | `U64` / `StrPtr` | |
| `__RTS_FN_NODE_BLOB_SLICE` | `Handle self, I64 start, I64 end, StrPtr type` | `Handle` | view, `Arc` clone + offset/len |
| `__RTS_FN_NODE_BLOB_ARRAY_BUFFER` / `_BYTES` / `_TEXT` | `Handle self` | `Handle` (a Promise handle — see §5.3) | |
| `__RTS_FN_NODE_BLOB_STREAM` | `Handle self` | `Handle` | deferred — needs the streams/web surface (§7) |
| `__RTS_FN_NODE_FILE_NEW` | `Handle parts_vec, StrPtr file_name, StrPtr endings, StrPtr mime_type, I64 last_modified_ms` | `Handle` | |
| `__RTS_FN_NODE_FILE_NAME` / `_LAST_MODIFIED` | `Handle self` | `StrPtr` / `I64` | |
| `__RTS_FN_NODE_BUFFER_RESOLVE_OBJECT_URL` | `StrPtr id` | `Handle` (0/sentinel if not found) | reads the cross-module object-URL registry, §5.7 |

`.ts` shim vs native extern split: **all class shape/prototype wiring**
(`class Buffer extends Uint8Array { ... }`, `class Blob { ... }`,
`class File extends Blob { ... }`, operator sugar like `buf[i]` indexing which
is already native TypedArray element access) lives in an rts-node `.ts`
prelude; **every byte-level operation** (read/write/compare/fill/encode/
decode/hash-free-copy) is a native extern per the table above. `atob`/`btoa`
are thin enough to be pure externs with no shim wrapper needed beyond binding
the global name.

### 5.3 Async model

- **`Buffer` and `File`'s own members: fully synchronous.** No event-loop or
  tokio involvement — every operation completes against already-resident
  memory before returning.
- **`Blob.arrayBuffer()` / `Blob.bytes()` / `Blob.text()`: Promise-returning
  but not genuinely asynchronous** — the data is already in memory (`Arc<[u8]>`),
  so these can settle **synchronously at creation** (an already-resolved
  Promise handle), matching real Node's microtask-deferred-but-not-I/O-bound
  behavior closely enough for spec fidelity. This still requires *a* Promise
  primitive to exist and be reachable from rts-node: the `Promise` **class**
  itself is primordial (engine/`rts-primitives`-owned), but the concrete
  "create an already-settled Promise handle" runtime primitive
  (`promise.create`/settle machinery) is documented as currently living in
  `rts-std::promise` + the event loop — see the blocking flag in §5.7. No
  `rt.spawn_blocking`/tokio call is needed for these three methods since there
  is no real async work to offload.
- **`Blob.stream()`** returns a Web `ReadableStream`; genuinely asynchronous
  consumption (backpressure, chunked pull) is out of scope for `node:buffer`
  itself and depends on the streams/web surface landing first (§7).
- **`transcode`/`isAscii`/`isUtf8`/`atob`/`btoa`/`resolveObjectURL`:** all
  synchronous, no Promise involved.

### 5.4 Multithread / worker interaction

- **`Buffer`/`ArrayBuffer` byte storage** follows the RTS threading model
  (`docs/specs/rts-threading-model.md`, per-thread regions + shared heap with
  promotion-on-publication): a `Buffer` allocated on one thread lives in that
  thread's region by default, exactly like Node (each worker has its own
  isolate/heap; a plain `Buffer` is **not** automatically shared). Real
  cross-thread sharing requires either (a) a `SharedArrayBuffer`-backed view,
  which maps directly onto the RTS `shared` heap primitive, or (b) an explicit
  `postMessage(buf, [buf.buffer])` transfer, which must map onto
  "promotion on publication" (the region's ownership of that `ArrayBuffer`
  handle moves to the shared heap / to the receiving thread's region, and the
  sender's view is detached — mirroring `arraybuffer_detach` already present
  in `rts-engine`).
- **`Blob`/`File`** are immutable after construction, which makes them a good
  fit for the shared-heap-promotion path: since their `Arc<[u8]>` bytes never
  mutate, a `Blob` handle **can** be safely promoted to the shared heap and
  referenced (not copied) by multiple threads/regions — this matches real
  Node's documented behavior that a `Blob` posted through a `MessagePort` is
  not eagerly copied and remains valid on both sides. `MessagePort` itself
  (the channel abstraction) is `worker_threads` surface, out of scope here,
  but `node:buffer`'s `Blob` design should be built with that future mapping
  in mind (no interior mutability, `Arc`-friendly).
- **The internal allocation pool backing `Buffer.allocUnsafe`/`from(string)`/
  `concat`** is **per-thread mutable state** (an offset into a shared pool
  buffer) and must be a `thread_local!` per the `02-runtime.md` "pattern for
  thread-local caches" — each RTS thread/worker gets its own independent pool,
  matching real Node where each worker thread (separate V8 isolate) has its
  own independently-pooled `Buffer` allocations. Pools must never be shared
  across threads without explicit synchronization.
- **The rts-node Blob/File handle table** (§5.1/§5.2) should be built
  shard-aware from the start (mirroring the engine's 32-shard `HandleTable`
  pattern) if `Blob` creation turns out to be a hot path under
  `worker_threads` fan-out; otherwise a single `Mutex`-guarded slab is
  sufficient for the P0 milestone.

### 5.5 Buffer / TypedArray interop

- `Buffer` **is** a `Uint8Array` value at the engine's representation level —
  no separate "Buffer object" wrapper exists underneath; the `.ts` class
  `Buffer extends Uint8Array` only adds prototype methods on top of the
  primordial TypedArray shape (`{ buffer: Handle<ArrayBuffer>, byteOffset: i64,
  length: i64 }` per the engine's already-lowered TypedArray element
  indexing). This means indexed access (`buf[i]`), `.length`, `.buffer`,
  `.byteOffset` need **zero** rts-node code — they are inherited for free from
  the primordial TypedArray. rts-node's added instance methods must accept a
  plain `Uint8Array` receiver too (not just a `Buffer`), matching Node's
  documented `Buffer.prototype.write.call(uint8array, ...)` support.
- **`Buffer.from(arrayBuffer, ...)` / `structuredClone(buf)` / transferable
  postMessage** all operate on the underlying `ArrayBuffer` handle directly
  via the engine's existing `arraybuffer_data_ptr`/`arraybuffer_byte_len`/
  `arraybuffer_detach` functions — rts-node adds no new memory-sharing
  primitive for this, it is pure reuse.
- **`Buffer.copyBytesFrom(view, ...)`** is the one place Node explicitly warns
  that passing a `Buffer`/TypedArray to another TypedArray's constructor
  copies **numeric elements**, not raw bytes; the rts-node extern must
  reinterpret the source's raw byte span (via `arraybuffer_data_ptr` +
  `byteOffset`/`length`), not iterate+widen/narrow element values.

### 5.6 Doctrine placement

- **Non-primordial, confirmed.** `Buffer`/`Blob`/`File` have no native literal
  syntax (`Buffer.from(...)`/`new Blob(...)` are calls, not literals), so per
  the "dividing line is native syntax" doctrine they are **not** engine
  primordials — only the `Uint8Array`/`ArrayBuffer`/`DataView`/`TypedArray`
  machinery `Buffer` is built on top of is primordial. The engine front-end
  must never hardcode the name `"Buffer"`, `"Blob"`, or `"File"`, and must not
  special-case the shape of `import ... from "node:buffer"`. Resolution flows
  entirely through rts-node's own data table (`NodespaceSpec` / `NODE_SPECS` /
  `node_lookup` / `ns_prefix_for`, currently in `crates/rts-node/src/lib.rs`) —
  `"node:buffer"` maps to an `ns_prefix` the same way every other `node:*`
  module does; no new engine-side special case is introduced by this module.
- **Global injection without hardcoding.** Real Node auto-injects `Buffer`,
  `Blob`, `File`, `atob`, `btoa` into every module's scope with no explicit
  import. RTS must reproduce this via the same **generic** ambient-global
  mechanism used for other backend globals (per `CLAUDE.md`'s ANTI-HARDCODE
  §3: "a whole global object/class... write it as a `.ts` PRELUDE and
  `e.include` it"): rts-node ships a `buffer_globals.ts` prelude that is
  unconditionally included for the Node target and binds `Buffer`/`Blob`/
  `File`/`atob`/`btoa` into global scope; the front-end's inclusion mechanism
  stays name-agnostic (it just includes a prelude file), so no
  `if name == "Buffer"` arm is ever written in the engine.
- **Where the `.ts` lives:** `crates/rts-node/src/buffer/*.ts` (rts-node owns
  all of `node:buffer`, unlike primordial `.ts` in `rts-primitives` or
  universal non-primordial `.ts` in `rts-shared/src/stdlib/`) — `node:buffer`
  is Node-specific surface, not a JS/TS-universal one, so it belongs
  exclusively under `rts-node`.

### 5.7 Shared-infra dependencies (FLAG)

- **Promise/microtask primitive** for `Blob.arrayBuffer()`/`bytes()`/`text()`
  (need to construct an already-settled Promise handle). The `Promise` class
  is primordial, but the concrete create/settle runtime machinery is
  documented today as living in `rts-std::promise` + the event loop
  (`async_rt`). Since `rts-node` cannot depend on `rts-std`, this settle
  primitive must be reachable from a lower shared crate (most likely hoisted
  into `rts-engine` or `rts-primitives`, where the `Promise` class itself
  already lives) before `Blob` can be implemented per spec. Not required for
  the rest of `node:buffer` (everything else is synchronous).
- **base64/hex codec duplication.** `rts-std::crypto` already implements
  base64 and hex encode/decode (for `crypto`/other namespaces). Per the
  owner's full-independence decision, `rts-node` must **either** vendor its
  own independent copy (accepted duplication, zero shared-crate risk) **or**
  the project could choose to hoist a tiny codec-only crate both `rts-std`
  and `rts-node` depend on. This spec assumes independent vendoring (no new
  shared crate) unless the owner decides otherwise.
- **Object-URL registry** backing `buffer.resolveObjectURL(id)` is populated
  by `URL.createObjectURL(blob)`, which is Web/URL-global surface, not part
  of `node:buffer` and not currently owned by any node-independent crate.
  This registry must live somewhere both the URL global implementation and
  `rts-node` can reach — flagged as a genuine cross-module design question,
  not solvable purely within `rts-node`.
- **MessagePort / structured-clone transfer plumbing** (needed for true
  zero-copy `Blob` sharing across `worker_threads`) does not exist yet in any
  crate; `node:buffer`'s `Blob` design (immutable, `Arc`-backed) is written to
  be forward-compatible with it, but implementing the transfer itself is
  `worker_threads` scope, not this module's.
- **tokio / shared async runtime:** **not required** for `node:buffer` — every
  method resolves without genuine I/O or cross-thread hand-off. Listed here
  only to explicitly rule it out, since the prompt's async-infra checklist
  calls for it to be addressed per module.
- **TLS/rustls, net sockets, crypto (SHA/CSPRNG):** **none** — not used by
  `node:buffer`.

### 5.8 Implementation phases

1. **(a)** Land `Buffer.alloc`/`allocUnsafe`/`allocUnsafeSlow` +
   `Buffer.from(string|array|buffer)` + `Buffer.isBuffer`/`isEncoding`/
   `byteLength` as externs over the existing `Entry::ArrayBuffer` primitive,
   plus the `.ts` `class Buffer extends Uint8Array` shim with no extra
   instance methods yet — proves the "Buffer is just a Uint8Array" mapping
   end to end.
2. **(b)** Add the numeric read/write family (44 externs) with correct
   bounds-checking (`ERR_OUT_OF_RANGE`) — highest per-symbol count but
   mechanical/uniform once one width is done.
3. **(c)** Add `toString`/`write` + the `utf8`/`latin1`/`ascii`/`hex` codecs
   (no crate dependency yet).
4. **(d)** Add `base64`/`base64url` codec (vendor `base64` crate) +
   `utf16le`/`ucs2` codec + `atob`/`btoa` (reuses the base64 codec).
5. **(e)** Add `compare`/`equals`/`includes`/`indexOf`/`lastIndexOf`/`fill`/
   `copy`/`slice`/`subarray`/`swap16`/`swap32`/`swap64`/`entries`/`keys`/
   `values`/`toJSON` + the static `Buffer.compare`/`concat`/`copyBytesFrom`.
6. **(f)** Wire the ambient-global prelude (`Buffer` usable with no import) +
   `new Buffer(...)` deprecated-constructor compat path.
7. **(g)** Implement `transcode`/`isAscii`/`isUtf8`.
8. **(h)** Implement `Blob` (construction, `size`/`type`, `slice`) with
   synchronous-settle stubs for `arrayBuffer`/`bytes`/`text` gated on the
   Promise-primitive dependency from §5.7 landing first.
9. **(i)** Implement `File` on top of `Blob`.
10. **(j)** Implement `resolveObjectURL` once the object-URL registry
    location is decided (§5.7/§7); otherwise ship it returning `undefined`
    always, documented as a known gap.
11. **(k)** Implement `INSPECT_MAX_BYTES`/`kMaxLength`/`kStringMaxLength`/
    `constants.*` + `util.inspect` integration for `console.log(buffer)` hex
    dumps.

---

## 6. Test plan

`tests/node/buffer/*.test.ts` (`rts:test` format):

- **Allocation:** `Buffer.alloc(10)` all-zero; `Buffer.alloc(5, 'ab')` repeats
  fill string; `Buffer.alloc(0)`; `Buffer.alloc(-1)` throws `ERR_OUT_OF_RANGE`;
  `Buffer.allocUnsafe(100)` has correct `.length`; `Buffer.alloc('x' as any)`
  throws `ERR_INVALID_ARG_TYPE`.
- **`Buffer.from` overloads:** from a plain array `[1,2,3]`; from a string
  with each supported encoding (`utf8`, `ascii`, `latin1`, `hex`, `base64`,
  `base64url`, `utf16le`); from another `Buffer` (copy semantics — mutating
  the copy does not affect the original); from an `ArrayBuffer` with
  `byteOffset`/`length` (view semantics — mutating the view **does** affect
  the backing `ArrayBuffer`); from an object with `valueOf()`; from an object
  with `[Symbol.toPrimitive]`; from a `Uint16Array` via `copyBytesFrom` vs the
  numeric-copy behavior of the plain constructor (both in the same test to
  demonstrate the documented incompatibility).
- **Round-trip encode/decode:** for every `BufferEncoding`, `Buffer.from(s, enc).toString(enc) === s` for representative strings including empty string, multi-byte UTF-8, and (for utf16le) an astral code point (surrogate pair).
- **Hex truncation edge cases:** `Buffer.from('1ag123', 'hex')`,
  `Buffer.from('1a7', 'hex')`, `Buffer.from('1634', 'hex')` matching the exact
  documented truncation behavior.
- **Base64url no-padding:** encode a length whose base64 form would need `=`
  padding and assert `base64url` output has none, while decode still accepts
  padded input.
- **Numeric read/write:** for each width (8/16/32/64-bit int, float, double,
  BE/LE) — write then read back; write near the type's min/max boundary;
  read/write at `offset = buf.length - width` (last valid position) and at
  `offset = buf.length - width + 1` (must throw `ERR_OUT_OF_RANGE`).
- **BigInt variants:** `writeBigInt64BE`/`readBigInt64BE` round-trip with a
  value exceeding 53-bit safe-integer range to prove no precision loss versus
  the plain `Int32`/`Number` path.
- **`compare`/`equals`/`includes`/`indexOf`/`lastIndexOf`:** empty buffer vs
  non-empty; needle longer than haystack; negative `byteOffset` (searches from
  end); needle as a `number` (single byte) vs `string` vs `Buffer`.
- **`slice`/`subarray` view semantics:** mutate a slice and assert the parent
  buffer's bytes changed too (proves view, not copy) — contrasted against a
  `Buffer.from(existingBuffer)` copy where mutation does **not** propagate.
- **`fill`:** fill with a string, a number, a `Buffer` pattern shorter than
  the target range (repeats); fill a non-zero-length buffer with a
  zero-length `fill` value → throws.
- **`swap16`/`swap32`/`swap64`:** correct byte-order reversal; wrong-length
  buffer throws `RangeError`.
- **`Buffer.concat`:** empty list → zero-length buffer; explicit
  `totalLength` smaller than the sum (truncates) and larger (zero-pads).
- **`Buffer.compare`/`Buffer.isBuffer`/`Buffer.isEncoding`:** including
  `Buffer.isEncoding('BASE64')` (case-insensitivity) and
  `Buffer.isEncoding('bogus')` → `false` (never throws).
- **`toJSON`:** `JSON.stringify(Buffer.from([1,2,3]))` produces
  `{"type":"Buffer","data":[1,2,3]}`.
- **Deprecated constructor compat:** `new Buffer([1,2,3])`,
  `new Buffer('hi')`, `new Buffer(10)` all behave like their `Buffer.from`/
  `Buffer.alloc` equivalents.
- **`Blob`:** empty `new Blob()` has `size === 0`; construct from mixed
  `string`/`ArrayBuffer`/nested `Blob` sources and assert `size` is the sum;
  `type` normalization/passthrough (no validation — garbage MIME type
  accepted verbatim); `slice(start, end)` returns the correct sub-range;
  `arrayBuffer()`/`bytes()`/`text()` all resolve (are genuinely thenable) with
  correct content; mutate the original source `ArrayBuffer` after
  construction and assert the `Blob` content is unaffected (copy-on-construct
  proof).
- **`File`:** `new File(['data'], 'a.txt')` has correct `name` and a recent
  `lastModified`; explicit `lastModified` option override; `File` inherits
  and correctly executes `Blob.prototype.slice`/`text`.
- **`atob`/`btoa`:** round-trip a binary string; `atob` on invalid base64
  throws; `btoa` on a string containing a code point > 255 throws.
- **`transcode`:** `latin1` → `utf8` and back; unsupported encoding name
  (e.g. `'base64'`) throws `ERR_UNKNOWN_ENCODING`.
- **`isAscii`/`isUtf8`:** ASCII-only buffer → `true`/`true`; buffer with a
  byte `>= 0x80` that is invalid UTF-8 → `false`/`false`; valid multi-byte
  UTF-8 (non-ASCII) → `false`/`true`.
- **Ambient global:** a `.test.ts` file that uses `Buffer.from(...)` with
  **no import statement at all**, proving the global-injection prelude works.
- **Multithread (worker_threads, once that module lands):** allocate a
  `Buffer` backed by a `SharedArrayBuffer` on the main thread, write from a
  worker, read the updated bytes on the main thread (shared-heap-promotion
  path); post a plain (non-shared) `Buffer`'s underlying `ArrayBuffer` with an
  explicit transfer list and assert the sender's view is detached
  (`byteLength === 0`) afterward; post a `Blob` through a `MessagePort`
  without a transfer list and assert both sides can independently call
  `.text()` with matching content (no eager copy required, but no corruption
  either).

## 7. Open questions / deferrals

- **`blob.stream()` / Web `ReadableStream`:** deferred until the
  streams/web surface exists; ship as `todo!()` or a minimal non-backpressured
  stub, documented as incomplete.
- **`resolveObjectURL`/`URL.createObjectURL` pairing:** needs an owner
  decision on where the shared object-URL registry lives (a new tiny shared
  crate? inside whichever crate implements the `URL` global?) before it can
  be implemented for real; ship returning `undefined` unconditionally until
  resolved.
- **Exact `atob`/`btoa` error type:** real browsers/Node throw a `DOMException`
  named `InvalidCharacterError`; RTS has no `DOMException` primordial today —
  decide whether to throw a plain `Error` with `.name = 'InvalidCharacterError'`
  or introduce `DOMException` as part of the wider Web-globals surface.
  Marked `(verify)` in §2.2.
- **Exact numeric values of `buffer.constants.MAX_LENGTH` /
  `MAX_STRING_LENGTH`** for RTS's own runtime (these are V8-internal
  implementation limits in real Node; RTS should pick its own principled
  ceiling — likely `i32::MAX` or an `ArrayBuffer`-handle-table-derived limit —
  rather than literally copying V8's numbers). Marked `(verify)`.
- **`ERR_BUFFER_OUT_OF_BOUNDS` exact trigger conditions:** could not be
  fully confirmed from the fetched docs; needs a pass against Node's actual
  `lib/buffer.js` source (or a differential test against real Node) before
  finalizing which zero-arg edge cases use this code versus plain
  `ERR_OUT_OF_RANGE`.
- **Structured-clone/transfer semantics for `Blob`/`File` via
  `structuredClone()`** (as opposed to `MessagePort`) are not covered by the
  fetched Node docs in detail; needs its own differential-test pass once
  `structuredClone` itself is implemented for non-primordial classes.
- **Buffer pool cross-request reuse under the HTTP server / thread-pool
  workloads:** worth a follow-up perf note once `node:http` lands, since
  Node's pool behavior has known GC-pressure tradeoffs (`allocUnsafeSlow`
  exists specifically to work around them) that RTS's own GC (mark+sweep,
  not generational yet) may interact with differently.
