# node:stream

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:stream` (+ submodules `node:stream/promises`, `node:stream/consumers`, `node:stream/web`) |
| Node.js version | 25.x (`https://nodejs.org/docs/latest-v25.x/api/stream.html`, `webstreams.html`) |
| Stability | 2 - Stable |
| Tier | P0 |
| Status | [~] Core implemented — `Readable`/`Writable`/`Duplex`/`Transform`/`PassThrough` (constructor-options + events + push/read/pause/resume/pipe/unpipe/write/end/cork/uncork/destroy + backpressure + objectMode + encoding + all state getters + async-iter helper methods) as an ambient `.ts` prelude; `pipeline`/`finished`/`compose`/`duplexPair`/`isErrored`/`isReadable`/`isWritable`/`addAbortSignal`/`get/setDefaultHighWaterMark`; `stream/consumers` (text/json/buffer/arrayBuffer/bytes/blob); `stream/promises`; `stream/web` re-exports the ambient WHATWG globals. Deferred (documented): `CompressionStream`/`DecompressionStream` (shared zlib/Brotli codec externs, tracked with `node:zlib`), WHATWG BYOB, real cross-thread transfer. **Deviation:** completion events emit synchronously (RTS's harness doesn't drain microtasks between top-level setup and assertions) — consumers attach listeners before data becomes available (standard paused→resume pattern). |
| Import forms | `import stream, { Readable, Writable, Duplex, Transform, PassThrough, pipeline, finished, compose, duplexPair, isErrored, isReadable, isWritable, addAbortSignal, getDefaultHighWaterMark, setDefaultHighWaterMark } from "node:stream"`; `import { pipeline, finished } from "node:stream/promises"`; `import { text, json, buffer, arrayBuffer, blob, bytes } from "node:stream/consumers"`; `import { ReadableStream, ReadableStreamDefaultReader, ReadableStreamBYOBReader, WritableStream, WritableStreamDefaultWriter, TransformStream, ByteLengthQueuingStrategy, CountQueuingStrategy, TextEncoderStream, TextDecoderStream, CompressionStream, DecompressionStream } from "node:stream/web"`; CJS `const stream = require('node:stream')`, `require('node:stream').promises` (same object as `node:stream/promises`), `require('node:stream/consumers')`, `require('node:stream/web')` |
| Globals exposed | `ReadableStream`, `ReadableStreamDefaultReader`, `ReadableStreamBYOBReader`, `WritableStream`, `WritableStreamDefaultWriter`, `TransformStream`, `ByteLengthQueuingStrategy`, `CountQueuingStrategy`, `TextEncoderStream`, `TextDecoderStream`, `CompressionStream`, `DecompressionStream` are ambient WHATWG globals on `globalThis` with **no import** (Node exposes them since v18); `node:stream/web` re-exports the *same* classes, mirroring the `node:events`/`EventTarget` pattern. `Readable`/`Writable`/`Duplex`/`Transform`/`PassThrough`/`Stream` are **not** globals — they require the `node:stream` import. |

## 1. Purpose

`node:stream` is Node's abstract interface for working with streaming data:
chunked, backpressure-aware producer/consumer objects that every I/O-adjacent
Node core module (`fs`, `net`, `http`, `zlib`, `crypto`, `child_process`,
`readline`) builds on. It defines four base classes — `Readable` (a data
source), `Writable` (a data sink), `Duplex` (both), `Transform` (a `Duplex`
that maps input to output) — plus `PassThrough` (a trivial `Transform`), and a
set of utility functions (`pipeline`, `finished`, `compose`, `addAbortSignal`,
…) that connect and observe streams safely without manual event wiring. It
also hosts three closely related submodules bundled in this spec:
`stream/promises` (promise-returning `pipeline`/`finished`), `stream/consumers`
(drain a stream fully into a `Buffer`/`string`/JSON value/`Blob`/`ArrayBuffer`),
and `stream/web` (the WHATWG Streams Standard — `ReadableStream`,
`WritableStream`, `TransformStream`, byte/BYOB streaming, queuing strategies,
`TextEncoderStream`/`TextDecoderStream`, `CompressionStream`/
`DecompressionStream` — the same API browsers and `fetch()` bodies use, which
Node also exposes as ambient globals). Almost every other Node core module RTS
targets (`fs.ReadStream`/`WriteStream`, `net.Socket`, `http.IncomingMessage`/
`ServerResponse`, `zlib.Gzip`/`Deflate`, `child_process` stdio) extends or
returns a `node:stream` class, making this a P0 foundation module like
`node:events` and `node:buffer`.

## 2. Exported API surface (COMPLETE)

### Classes

#### `class stream.Stream extends EventEmitter`

Legacy base class (predates `Readable`/`Writable`). No constructor options of
its own; kept for backward compatibility and `instanceof stream.Stream`
checks (every `Readable`/`Writable`/`Duplex`/`Transform`/`PassThrough`
instance is also a `Stream`). RTS does not need to give it any behavior beyond
being the common prototype ancestor — see §5.1.

#### `class stream.Readable extends Stream`

A readable data source. Base class for `fs.ReadStream`, `http.IncomingMessage`,
`net.Socket` (readable half), `zlib.Gunzip`, etc.

**Constructor**

```ts
new stream.Readable(options?: ReadableOptions)
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `ReadableOptions` | yes | `{}` |

Throws: none at construction; a malformed `options.encoding` throws
`ERR_UNKNOWN_ENCODING` lazily on first use via `setEncoding`.

**Events**

| Event | Args | Notes |
|---|---|---|
| `'close'` | `()` | Emitted after `destroy()` (if `emitClose` is `true`, the default) once all underlying resources are released. |
| `'data'` | `(chunk: Buffer \| string \| any)` | Only emitted in **flowing mode** (after `.pipe()`, `.on('data', …)`, or `.resume()`). Switches the stream to flowing mode as a side effect of attaching the listener. |
| `'end'` | `()` | No more data will ever be produced; only in flowing mode / after the buffer is fully drained via `.read()`. |
| `'error'` | `(err: Error)` | Should not be followed by `'close'` unless `emitClose`/`autoDestroy` triggers it; a `Readable` implementation should not emit more than once. |
| `'pause'` | `()` | Emitted when `.pause()` is called or the stream drops out of flowing mode. |
| `'readable'` | `()` | Data available to `.read()`, or the end has been reached (`.read()` will then return `null` and emit `'end'`). |
| `'resume'` | `()` | Emitted when `.resume()` is called and the stream is not already flowing. |

**Instance methods**

| Signature | Returns | Variant | Notes |
|---|---|---|---|
| `readable.destroy(error?: Error)` | `this` | sync | Emits `'error'` (if `error` given) then `'close'` (unless `emitClose: false`). Idempotent — a second call is a no-op. |
| `readable.isPaused(): boolean` | `boolean` | sync | `true` in default/paused mode; `false` once flowing. |
| `readable.pause(): this` | `this` | sync | Stops `'data'` from flowing; buffered data accumulates up to `highWaterMark`. |
| `readable.pipe(destination: Writable, options?: { end?: boolean }): Writable` | the `destination` argument | sync | Switches to flowing mode; `options.end` (default `true`) controls whether `destination.end()` is called on source `'end'`. Returns `destination` to allow chaining `a.pipe(b).pipe(c)`. |
| `readable.read(size?: number): any` | `Buffer \| string \| null \| any` | sync | `null` when no data available or stream ended. In object mode `size` is ignored. |
| `readable.resume(): this` | `this` | sync | Switches to flowing mode, discarding data if no `'data'` listener is attached ("resume and discard"). |
| `readable.setEncoding(encoding: BufferEncoding): this` | `this` | sync | Chunks delivered as decoded strings instead of `Buffer`; throws `ERR_UNKNOWN_ENCODING` for an unrecognized encoding. |
| `readable.unpipe(destination?: Writable): this` | `this` | sync | Detaches one or (if omitted) all piped destinations. |
| `readable.unshift(chunk: any, encoding?: BufferEncoding): void` | `void` | sync | Pushes a chunk back onto the *front* of the read queue; throws `ERR_STREAM_UNSHIFT_AFTER_END_EVENT` if called after `'end'`. |
| `readable.wrap(stream: EventEmitter): this` | `this` | sync | Adapts a pre-streams2 (`'data'`/`'pause'`/`'resume'` emitting) legacy stream into a modern `Readable`. |
| `readable.compose(stream: Duplex \| AsyncGeneratorFunction, options?: { signal?: AbortSignal }): Duplex` | `Duplex` | sync | v17.0.0+. Pipes `this` into `stream` and returns the composed result as a new `Duplex`. |
| `readable.iterator(options?: { destroyOnReturn?: boolean }): AsyncIterableIterator<any>` | async iterator | sync (returns iterator) | v16.3.0+. `destroyOnReturn` (default `true`) controls whether breaking out of a `for await` loop destroys the source. |
| `readable.map(fn: (chunk: any, options?: { signal: AbortSignal }) => any \| Promise<any>, options?: { concurrency?: number, highWaterMark?: number, signal?: AbortSignal }): Readable` | `Readable` | sync (returns stream) | v17.0.0+. `concurrency` default `1`. |
| `readable.filter(fn: (chunk: any, options?: { signal: AbortSignal }) => boolean \| Promise<boolean>, options?: { concurrency?: number, signal?: AbortSignal }): Readable` | `Readable` | sync (returns stream) | v17.0.0+. |
| `readable.forEach(fn: (chunk: any, options?: { signal: AbortSignal }) => void \| Promise<void>, options?: { concurrency?: number, signal?: AbortSignal }): Promise<void>` | `Promise<void>` | promise | v17.0.0+. |
| `readable.toArray(options?: { signal?: AbortSignal }): Promise<any[]>` | `Promise<any[]>` | promise | v17.0.0+. |
| `readable.some(fn: (chunk: any, options?: { signal: AbortSignal }) => boolean \| Promise<boolean>, options?: { concurrency?: number, signal?: AbortSignal }): Promise<boolean>` | `Promise<boolean>` | promise | v17.0.0+. Short-circuits on first `true`. |
| `readable.find(fn: (chunk: any, options?: { signal: AbortSignal }) => boolean \| Promise<boolean>, options?: { concurrency?: number, signal?: AbortSignal }): Promise<any \| undefined>` | `Promise<any \| undefined>` | promise | v17.0.0+. |
| `readable.every(fn: (chunk: any, options?: { signal: AbortSignal }) => boolean \| Promise<boolean>, options?: { concurrency?: number, signal?: AbortSignal }): Promise<boolean>` | `Promise<boolean>` | promise | v17.0.0+. Short-circuits on first `false`. |
| `readable.flatMap(fn: (chunk: any, options?: { signal: AbortSignal }) => any \| Iterable<any> \| AsyncIterable<any>, options?: { concurrency?: number, signal?: AbortSignal }): Readable` | `Readable` | sync (returns stream) | v17.0.0+. |
| `readable.drop(limit: number, options?: { signal?: AbortSignal }): Readable` | `Readable` | sync (returns stream) | v17.0.0+. |
| `readable.take(limit: number, options?: { signal?: AbortSignal }): Readable` | `Readable` | sync (returns stream) | v17.0.0+. |
| `readable.reduce(fn: (previous: any, chunk: any, options?: { signal: AbortSignal }) => any \| Promise<any>, initial?: any, options?: { signal?: AbortSignal }): Promise<any>` | `Promise<any>` | promise | v17.0.0+. `TypeError` if no `initial` and the stream is empty. |
| `readable[Symbol.asyncIterator](): AsyncIterableIterator<any>` | async iterator | sync (returns iterator) | Enables `for await (const chunk of readable)`. Destroys the stream if the loop exits early (see `.iterator()`). |
| `readable[Symbol.asyncDispose](): Promise<void>` | `Promise<void>` | promise | v22.4.0+. Enables `await using r = createReadable()`; calls `.destroy()` and waits for `'close'`. |

**Instance properties (all read-only unless noted)**

| Property | Type | Notes |
|---|---|---|
| `readable.closed` | `boolean` | `true` after `'close'` has been emitted. |
| `readable.destroyed` | `boolean` | `true` after `.destroy()` has been called. |
| `readable.errored` | `Error \| null` | The error passed to `.destroy(err)`, else `null`. |
| `readable.readable` | `boolean` | `true` if it is safe to call `.read()`. |
| `readable.readableAborted` | `boolean` | v16.8.0+. `true` if destroyed before `'end'`. |
| `readable.readableDidRead` | `boolean` | v16.7.0+. `true` once a `'data'` event has fired at least once. |
| `readable.readableEncoding` | `BufferEncoding \| null` | Set via `.setEncoding()`. |
| `readable.readableEnded` | `boolean` | `true` after `'end'` has been emitted. |
| `readable.readableFlowing` | `boolean \| null` | `null` before any consumer attaches; `true`/`false` once flowing state is determined. |
| `readable.readableHighWaterMark` | `number` | From constructor `options.highWaterMark`. |
| `readable.readableLength` | `number` | Bytes (or objects, in object mode) currently buffered. |
| `readable.readableObjectMode` | `boolean` | From constructor `options.objectMode`. |

**Methods for implementers (overridden by subclasses, not called by consumers)**

| Signature | Notes |
|---|---|
| `readable._construct(callback: (error?: Error) => void): void` | Optional; deferred initialization before the first `_read`/`_destroy`. |
| `readable._read(size: number): void` | **Required.** Must eventually call `.push()`. Throws `ERR_METHOD_NOT_IMPLEMENTED` if never overridden and invoked. |
| `readable._destroy(error: Error \| null, callback: (error?: Error \| null) => void): void` | Optional cleanup hook. |
| `readable.push(chunk: any, encoding?: BufferEncoding): boolean` | Called *by* the implementer inside `_read`. Returns `false` when the internal buffer has reached `highWaterMark` (a hint to stop pushing). `push(null)` signals EOF. Throws `ERR_STREAM_PUSH_AFTER_EOF` if called again after `push(null)`. |

**Static methods**

| Signature | Returns | Notes |
|---|---|---|
| `Readable.from(iterable: Iterable<any> \| AsyncIterable<any>, options?: ReadableOptions): Readable` | `Readable` | Wraps any (async) iterable, including generators and generator functions' return values. |
| `Readable.fromWeb(readableStream: streamWeb.ReadableStream, options?: { encoding?: BufferEncoding, highWaterMark?: number, objectMode?: boolean, signal?: AbortSignal }): Readable` | `Readable` | v17.0.0+. Bridges a WHATWG `ReadableStream` into a Node `Readable`. |
| `Readable.isDisturbed(stream: Readable \| streamWeb.ReadableStream): boolean` | `boolean` | v16.8.0+. `true` if the stream has been read from or errored. |
| `Readable.toWeb(streamReadable: Readable, options?: { strategy?: QueuingStrategy }): streamWeb.ReadableStream` | `streamWeb.ReadableStream` | v17.0.0+. |

#### `class stream.Writable extends Stream`

A writable data sink. Base class for `fs.WriteStream`, `http.ServerResponse`,
`net.Socket` (writable half), `zlib.Gzip`, etc.

**Constructor**

```ts
new stream.Writable(options?: WritableOptions)
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `WritableOptions` | yes | `{}` |

Throws: none at construction.

**Events**

| Event | Args | Notes |
|---|---|---|
| `'close'` | `()` | After `destroy()`, once resources are released (`emitClose`, default `true`). |
| `'drain'` | `()` | Safe to resume writing after `.write()` returned `false`. |
| `'error'` | `(err: Error)` | Should be followed by `'close'` unless `autoDestroy: false`; must not be emitted twice. |
| `'finish'` | `()` | After `.end()` and all data has been flushed to the underlying system. |
| `'pipe'` | `(src: Readable)` | Emitted on this `Writable` when a `Readable` is `.pipe()`d into it. |
| `'unpipe'` | `(src: Readable)` | Emitted when `.unpipe()` is called on a source, or the source emits `'error'`. |

**Instance methods**

| Signature | Returns | Variant | Notes |
|---|---|---|---|
| `writable.cork(): void` | `void` | sync | Buffers all writes in memory until `.uncork()`. Nestable (matched calls required). |
| `writable.destroy(error?: Error): this` | `this` | sync | Idempotent. |
| `writable.end(chunk?: any, encoding?: BufferEncoding, callback?: (error?: Error \| null) => void): this` | `this` | sync (callback fires async) | Signals no more data will be written; `callback` is attached once for `'finish'`/`'error'`. Overloads: `.end()`, `.end(chunk)`, `.end(chunk, encoding)`, `.end(callback)`, `.end(chunk, callback)`. |
| `writable.setDefaultEncoding(encoding: BufferEncoding): this` | `this` | sync | Throws `ERR_UNKNOWN_ENCODING` for an unrecognized encoding. |
| `writable.uncork(): void` | `void` | sync | Flushes data buffered by `.cork()`; if called fewer times than `.cork()`, remains corked (reference-counted). |
| `writable.write(chunk: any, encoding?: BufferEncoding, callback?: (error?: Error \| null) => void): boolean` | `boolean` | sync (callback fires async) | Returns `false` if the internal buffer exceeds `highWaterMark` (**backpressure signal** — wait for `'drain'`). Throws `ERR_STREAM_WRITE_AFTER_END` after `.end()`; throws `ERR_STREAM_NULL_VALUES` for `null` chunk. |
| `writable[Symbol.asyncDispose](): Promise<void>` | `Promise<void>` | promise | v22.4.0+. Calls `.end()` (or `.destroy()` if an error is pending) and waits for `'finish'`/`'close'`. |

**Instance properties (read-only unless noted)**

| Property | Type | Notes |
|---|---|---|
| `writable.closed` | `boolean` | After `'close'`. |
| `writable.destroyed` | `boolean` | After `.destroy()`. |
| `writable.errored` | `Error \| null` | |
| `writable.writable` | `boolean` | `true` if safe to call `.write()`. |
| `writable.writableAborted` | `boolean` | v18.0.0+. `true` if destroyed before `'finish'`. |
| `writable.writableCorked` | `number` | Current cork depth (0 = not corked). |
| `writable.writableEnded` | `boolean` | `true` once `.end()` has been called (data may still be flushing). |
| `writable.writableFinished` | `boolean` | `true` once `'finish'` has been emitted. |
| `writable.writableHighWaterMark` | `number` | From constructor. |
| `writable.writableLength` | `number` | Bytes (or objects) currently queued to write. |
| `writable.writableNeedDrain` | `boolean` | v15.2.0+. `true` iff the last `.write()` returned `false` and `'drain'` has not fired yet. |
| `writable.writableObjectMode` | `boolean` | From constructor. |

**Methods for implementers**

| Signature | Notes |
|---|---|
| `writable._construct(callback: (error?: Error) => void): void` | Optional deferred init. |
| `writable._write(chunk: any, encoding: BufferEncoding, callback: (error?: Error \| null) => void): void` | **Required** unless `_writev` is provided. Must call `callback` **exactly once**; a second call throws `ERR_MULTIPLE_CALLBACK`. |
| `writable._writev(chunks: Array<{ chunk: any, encoding: BufferEncoding }>, callback: (error?: Error \| null) => void): void` | Optional batched-write path used when multiple writes are queued (e.g. while corked). |
| `writable._destroy(error: Error \| null, callback: (error?: Error \| null) => void): void` | Optional cleanup hook. |
| `writable._final(callback: (error?: Error \| null) => void): void` | Optional; invoked before `'finish'`, after all writes flush and `.end()` was called with no more pending writes. |

**Static methods**

| Signature | Returns | Notes |
|---|---|---|
| `Writable.fromWeb(writableStream: streamWeb.WritableStream, options?: { decodeStrings?: boolean, highWaterMark?: number, objectMode?: boolean, signal?: AbortSignal }): Writable` | `Writable` | v17.0.0+. |
| `Writable.toWeb(streamWritable: Writable): streamWeb.WritableStream` | `streamWeb.WritableStream` | v17.0.0+. |

#### `class stream.Duplex extends stream.Readable` (also implements the `Writable` instance surface)

Both a data source and a data sink over the **same** underlying resource (e.g.
a TCP socket). Node implements `Duplex` by inheriting `Readable` and mixing in
the `Writable` prototype methods/properties (`cork`/`uncork`/`write`/`end`/…)
— from a TS-consumer perspective it exposes the full union of both APIs
documented above, unchanged.

**Constructor**

```ts
new stream.Duplex(options?: DuplexOptions)
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `DuplexOptions` | yes | `{}` |

**Inherits**: every `Readable` instance method/property/event **and** every
`Writable` instance method/property/event listed above, unmodified.

**Instance property (Duplex-specific)**

| Property | Type | Notes |
|---|---|---|
| `duplex.allowHalfOpen` | `boolean` | From constructor `options.allowHalfOpen` (default `true`). When `false`, the writable side auto-ends as soon as the readable side ends (and vice versa) — mimics classic Unix half-close-disabled socket behavior. |

**Static methods**

| Signature | Returns | Notes |
|---|---|---|
| `Duplex.from(src: Stream \| Blob \| ArrayBuffer \| string \| Iterable<any> \| AsyncIterable<any> \| AsyncGeneratorFunction \| Promise<any> \| Object): Duplex` | `Duplex` | v16.1.0+. `Object` form: `{ readable, writable }` pair. |
| `Duplex.fromWeb(pair: { readable: streamWeb.ReadableStream, writable: streamWeb.WritableStream }, options?: { allowHalfOpen?: boolean, decodeStrings?: boolean, encoding?: BufferEncoding, highWaterMark?: number, objectMode?: boolean, signal?: AbortSignal }): Duplex` | `Duplex` | v17.0.0+. |
| `Duplex.toWeb(streamDuplex: Duplex): { readable: streamWeb.ReadableStream, writable: streamWeb.WritableStream }` | `{ readable, writable }` | v17.0.0+. |

#### `class stream.Transform extends stream.Duplex`

A `Duplex` where output is programmatically related to input — write side and
read side are the same logical byte/object flow after a transform function.
Base class for `zlib.Gzip`/`Deflate`/`Brotli*`, `crypto.Cipher`/`Hash`.

**Constructor**

```ts
new stream.Transform(options?: TransformOptions)
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `TransformOptions` | yes | `{}` |

**Inherits**: all `Duplex` (= `Readable` + `Writable`) instance surface.

**Events** (same set as `Duplex`; notable coupling)

| Event | Notes |
|---|---|
| `'end'` | Fires once the writable side has received `'end'` from its source and all buffered output has been read. |
| `'finish'` | Fires once the readable side's output has been fully consumed after `_flush` completes. |

**Methods for implementers**

| Signature | Notes |
|---|---|
| `transform._transform(chunk: any, encoding: BufferEncoding, callback: (error?: Error \| null, data?: any) => void): void` | **Required.** Replaces `_write`+`_read`; call `callback(null, transformedChunk)` (or omit `data` to produce no output for this input chunk) exactly once. Calling `_transform` again before the previous callback fires throws `ERR_TRANSFORM_ALREADY_TRANSFORMING`. |
| `transform._flush(callback: (error?: Error \| null, data?: any) => void): void` | Optional; called once, after the writable side ends, before `'finish'` — a chance to emit trailing output (e.g. a compression footer). |

#### `class stream.PassThrough extends stream.Transform`

**Constructor**

```ts
new stream.PassThrough(options?: TransformOptions)
```

Identical `TransformOptions` shape as `Transform` (rarely used beyond
`objectMode`/`highWaterMark`). Behavior: `_transform` is pre-implemented as
`callback(null, chunk)` — every input chunk is forwarded unchanged. Useful for
observing/tee-ing a pipeline, or as a generic in-memory buffer stage.

---

### `node:stream/web` classes (WHATWG Streams Standard)

#### `class ReadableStream`

```ts
new ReadableStream(underlyingSource?: UnderlyingSource, strategy?: QueuingStrategy)
```

| Property | Type | Notes |
|---|---|---|
| `readableStream.locked` | `boolean` (read-only) | `true` while a reader is attached via `getReader()`. |

| Method | Returns | Notes |
|---|---|---|
| `readableStream.cancel(reason?: any): Promise<undefined>` | `Promise<undefined>` | Calls the source's `cancel(reason)`. |
| `readableStream.getReader(options?: { mode?: 'byob' }): ReadableStreamDefaultReader \| ReadableStreamBYOBReader` | reader | `mode: 'byob'` requires `type: 'bytes'` at construction. |
| `readableStream.pipeThrough(transform: { readable: ReadableStream, writable: WritableStream }, options?: StreamPipeOptions): ReadableStream` | `ReadableStream` | |
| `readableStream.pipeTo(destination: WritableStream, options?: StreamPipeOptions): Promise<undefined>` | `Promise<undefined>` | |
| `readableStream.tee(): [ReadableStream, ReadableStream]` | 2-tuple | Splits into two independently-readable branches over the same source. |
| `readableStream.values(options?: { preventCancel?: boolean }): AsyncIterableIterator<any>` | async iterator | |
| `readableStream[Symbol.asyncIterator]()` | async iterator | Alias of `.values()`. |
| `ReadableStream.from(iterable: Iterable<any> \| AsyncIterable<any>): ReadableStream` | `ReadableStream` (static) | Added v20.6.0. |

#### `class ReadableStreamDefaultReader`

```ts
new ReadableStreamDefaultReader(stream: ReadableStream)
```

| Property | Type |
|---|---|
| `reader.closed` | `Promise<undefined>` (read-only) |

| Method | Returns |
|---|---|
| `reader.cancel(reason?: any): Promise<undefined>` | `Promise<undefined>` |
| `reader.read(): Promise<{ value: any, done: boolean }>` | `Promise<{value, done}>` |
| `reader.releaseLock(): void` | `void` |

#### `class ReadableStreamBYOBReader`

```ts
new ReadableStreamBYOBReader(stream: ReadableStream)
```

| Property | Type |
|---|---|
| `reader.closed` | `Promise<undefined>` (read-only) |

| Method | Returns | Notes |
|---|---|---|
| `reader.cancel(reason?: any): Promise<undefined>` | `Promise<undefined>` | |
| `reader.read(view: ArrayBufferView, options?: { min?: number }): Promise<{ value: ArrayBufferView, done: boolean }>` | `Promise<{value, done}>` | Zero-copy: reads directly into caller-supplied `view`. `options.min` (Node addition) waits for at least that many bytes. |
| `reader.releaseLock(): void` | `void` | |

#### `class ReadableStreamDefaultController`

| Property | Type |
|---|---|
| `controller.desiredSize` | `number \| null` (read-only) |

| Method | Returns |
|---|---|
| `controller.close(): void` | `void` |
| `controller.enqueue(chunk?: any): void` | `void` |
| `controller.error(error?: any): void` | `void` |

#### `class ReadableByteStreamController`

| Property | Type |
|---|---|
| `controller.byobRequest` | `ReadableStreamBYOBRequest \| null` (read-only) |
| `controller.desiredSize` | `number \| null` (read-only) |

| Method | Returns |
|---|---|
| `controller.close(): void` | `void` |
| `controller.enqueue(chunk: ArrayBufferView): void` | `void` |
| `controller.error(error?: any): void` | `void` |

#### `class ReadableStreamBYOBRequest`

| Property | Type |
|---|---|
| `request.view` | `ArrayBufferView \| null` (read-only) |

| Method | Returns |
|---|---|
| `request.respond(bytesWritten: number): void` | `void` |
| `request.respondWithNewView(view: ArrayBufferView): void` | `void` |

#### `class WritableStream`

```ts
new WritableStream(underlyingSink?: UnderlyingSink, strategy?: QueuingStrategy)
```

| Property | Type |
|---|---|
| `writableStream.locked` | `boolean` (read-only) |

| Method | Returns |
|---|---|
| `writableStream.abort(reason?: any): Promise<undefined>` | `Promise<undefined>` |
| `writableStream.close(): Promise<undefined>` | `Promise<undefined>` |
| `writableStream.getWriter(): WritableStreamDefaultWriter` | `WritableStreamDefaultWriter` |

#### `class WritableStreamDefaultWriter`

```ts
new WritableStreamDefaultWriter(stream: WritableStream)
```

| Property | Type |
|---|---|
| `writer.closed` | `Promise<undefined>` (read-only) |
| `writer.desiredSize` | `number \| null` (read-only) |
| `writer.ready` | `Promise<undefined>` (read-only) — resolves when backpressure clears |

| Method | Returns |
|---|---|
| `writer.abort(reason?: any): Promise<undefined>` | `Promise<undefined>` |
| `writer.close(): Promise<undefined>` | `Promise<undefined>` |
| `writer.releaseLock(): void` | `void` |
| `writer.write(chunk?: any): Promise<undefined>` | `Promise<undefined>` |

#### `class WritableStreamDefaultController`

| Property | Type |
|---|---|
| `controller.signal` | `AbortSignal` (read-only) — aborts if the stream is aborted |

| Method | Returns |
|---|---|
| `controller.error(error?: any): void` | `void` |

#### `class TransformStream`

```ts
new TransformStream(transformer?: Transformer, writableStrategy?: QueuingStrategy, readableStrategy?: QueuingStrategy)
```

| Property | Type |
|---|---|
| `transformStream.readable` | `ReadableStream` (read-only) |
| `transformStream.writable` | `WritableStream` (read-only) |

#### `class TransformStreamDefaultController`

| Property | Type |
|---|---|
| `controller.desiredSize` | `number \| null` (read-only) |

| Method | Returns |
|---|---|
| `controller.enqueue(chunk?: any): void` | `void` |
| `controller.error(reason?: any): void` | `void` |
| `controller.terminate(): void` | `void` |

#### `class ByteLengthQueuingStrategy`

```ts
new ByteLengthQueuingStrategy(init: { highWaterMark: number })
```

| Property | Type |
|---|---|
| `strategy.highWaterMark` | `number` (read-only) |
| `strategy.size` | `(chunk: ArrayBufferView) => number` (read-only) — returns `chunk.byteLength` |

#### `class CountQueuingStrategy`

```ts
new CountQueuingStrategy(init: { highWaterMark: number })
```

| Property | Type |
|---|---|
| `strategy.highWaterMark` | `number` (read-only) |
| `strategy.size` | `(chunk: any) => number` (read-only) — always returns `1` |

#### `class TextEncoderStream`

```ts
new TextEncoderStream()
```

| Property | Type |
|---|---|
| `encoderStream.encoding` | `'utf-8'` (read-only) |
| `encoderStream.readable` | `ReadableStream<Uint8Array>` (read-only) |
| `encoderStream.writable` | `WritableStream<string>` (read-only) |

#### `class TextDecoderStream`

```ts
new TextDecoderStream(encoding?: string, options?: { fatal?: boolean, ignoreBOM?: boolean })
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `encoding` | `string` | yes | `'utf-8'` |
| `options.fatal` | `boolean` | yes | `false` |
| `options.ignoreBOM` | `boolean` | yes | `false` |

| Property | Type |
|---|---|
| `decoderStream.encoding` | `string` (read-only) |
| `decoderStream.fatal` | `boolean` (read-only) |
| `decoderStream.ignoreBOM` | `boolean` (read-only) |
| `decoderStream.readable` | `ReadableStream<string>` (read-only) |
| `decoderStream.writable` | `WritableStream<Uint8Array>` (read-only) |

#### `class CompressionStream`

```ts
new CompressionStream(format: 'deflate' | 'deflate-raw' | 'gzip' | 'brotli')
```

`'brotli'` is a Node-specific extension beyond the 3-format WHATWG standard
**(verify exact Node version 'brotli' support landed — flagged in §7)**.

| Property | Type |
|---|---|
| `compressionStream.readable` | `ReadableStream<Uint8Array>` (read-only) |
| `compressionStream.writable` | `WritableStream<ArrayBufferView>` (read-only) |

#### `class DecompressionStream`

```ts
new DecompressionStream(format: 'deflate' | 'deflate-raw' | 'gzip' | 'brotli')
```

Same properties as `CompressionStream`, reversed direction.

---

### Top-level functions

#### `stream.pipeline(...)`

Two call shapes, both callback-based in the base `node:stream` module (the
promise form lives in `stream/promises`, re-documented below):

```ts
stream.pipeline(source: Streamable, ...transforms: Streamable[], destination: Streamable, callback: (error?: Error | null) => void): Streamable
stream.pipeline(streams: Streamable[], callback: (error?: Error | null) => void): Streamable
```

`Streamable = Stream | Iterable<any> | AsyncIterable<any> | (...) => (Stream | Iterable | AsyncIterable | Promise<any>)`

| Param | Type | Optional | Default |
|---|---|---|---|
| `source` / `streams[0]` | `Streamable` | no | — |
| `...transforms` | `Streamable[]` | yes | `[]` |
| `destination` / `streams[last]` | `Streamable` | no | — |
| `callback` | `(error?: Error \| null) => void` | no | — |

Returns: the last stream in the pipeline (the `destination`), for chaining.
Variant: **callback**. Throws (via `callback(err)`, never synchronously):
propagates the first error from any stream in the chain; destroys every other
stream in the chain when one errors or is destroyed early
(`ERR_STREAM_PREMATURE_CLOSE` if a stream closes before finishing without an
explicit error).

#### `stream.finished(stream, options?, callback)`

```ts
stream.finished(streamOrWebStream: Readable | Writable | Duplex | streamWeb.ReadableStream | streamWeb.WritableStream, options?: FinishedOptions, callback: (error?: Error | null) => void): () => void
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `streamOrWebStream` | stream-like | no | — |
| `options` | `FinishedOptions` | yes | `{}` |
| `callback` | `(error?: Error \| null) => void` | no | — |

Returns: a cleanup function that removes all listeners `finished()`
registered (call it to unsubscribe early without invoking `callback`).
Variant: **callback**.

#### `stream.compose(...streams)`

```ts
stream.compose(...streams: Array<Duplex | Readable | Writable | Iterable<any> | AsyncIterable<any> | AsyncGeneratorFunction>): Duplex
```

Returns: a `Duplex` (v16.9.0+) combining all provided streams/functions into a
single stream — writes go into the first, reads come out of the last, errors
on any stage propagate and destroy the whole chain.

#### `stream.duplexPair(options?)`

```ts
stream.duplexPair(options?: DuplexOptions): [Duplex, Duplex]
```

Returns: a 2-tuple of `Duplex` streams (v15.0.0+) where writes to one appear
as reads on the other and vice versa (an in-memory socket pair, useful for
testing).

#### `stream.isErrored(stream)`

```ts
stream.isErrored(stream: Readable | Writable | Duplex | streamWeb.ReadableStream | streamWeb.WritableStream): boolean
```

Returns: `boolean` (v17.0.0+). Variant: sync.

#### `stream.isReadable(stream)`

```ts
stream.isReadable(stream: Readable | streamWeb.ReadableStream): boolean
```

Returns: `boolean` (v17.0.0+). Variant: sync.

#### `stream.isWritable(stream)`

```ts
stream.isWritable(stream: Writable | streamWeb.WritableStream): boolean
```

Returns: `boolean` (v17.9.0+/v16.19.0+). Variant: sync.

#### `stream.addAbortSignal(signal, stream)`

```ts
stream.addAbortSignal<T extends Stream>(signal: AbortSignal, stream: T): T
```

| Param | Type | Optional |
|---|---|---|
| `signal` | `AbortSignal` | no |
| `stream` | any stream | no |

Returns: the same `stream` argument (v15.0.0+), for chaining. Variant: sync
(the abort effect itself is async). When `signal` aborts, `stream.destroy()`
is called with an `AbortError`.

#### `stream.getDefaultHighWaterMark(objectMode)`

```ts
stream.getDefaultHighWaterMark(objectMode: boolean): number
```

Returns: `number` (v19.9.0+) — `16` if `objectMode`, else `16384`, unless
overridden process-wide by `setDefaultHighWaterMark`. Variant: sync.

#### `stream.setDefaultHighWaterMark(objectMode, value)`

```ts
stream.setDefaultHighWaterMark(objectMode: boolean, value: number): void
```

Returns: `void` (v19.9.0+). Variant: sync. Sets the **process-wide** default
used by every subsequently constructed stream that doesn't pass an explicit
`highWaterMark`.

---

### `node:stream/promises` (re-exported at `stream.promises` too)

#### `promises.pipeline(...)`

```ts
promisesPipeline(source: Streamable, ...transforms: Streamable[], destination: Streamable, options?: PipelineOptions): Promise<void>
promisesPipeline(streams: Streamable[], options?: PipelineOptions): Promise<void>
```

Returns: `Promise<void>`, fulfilled when the pipeline completes, rejected with
the first error. Variant: **promise**. Same semantics as `stream.pipeline`'s
callback form, plus `options.signal` (`AbortSignal`) and `options.end`
(`boolean`, default `true`, v18.0.0+ — whether the final destination is
`.end()`ed).

#### `promises.finished(stream, options?)`

```ts
promisesFinished(stream: Readable | Writable | Duplex | streamWeb.ReadableStream | streamWeb.WritableStream, options?: FinishedOptions): Promise<void>
```

Returns: `Promise<void>`. Variant: **promise**. `options.cleanup` (default
`false`, v19.1.0+) removes the internal listeners once the promise settles —
recommended `true` in most call sites to avoid dangling `'error'`/`'end'`/
`'finish'`/`'close'` listeners.

---

### `node:stream/consumers`

All six functions share the same shape: fully drain a `Readable`/
`ReadableStream`/`AsyncIterator` and return one Promise-wrapped
representation of the concatenated data.

| Signature | Returns | Variant |
|---|---|---|
| `consumers.arrayBuffer(stream: Readable \| streamWeb.ReadableStream \| AsyncIterator<any>): Promise<ArrayBuffer>` | `Promise<ArrayBuffer>` | promise |
| `consumers.blob(stream: ...): Promise<Blob>` | `Promise<Blob>` | promise |
| `consumers.buffer(stream: ...): Promise<Buffer>` | `Promise<Buffer>` | promise |
| `consumers.bytes(stream: ...): Promise<Uint8Array>` | `Promise<Uint8Array>` | promise — added v25.6.0 |
| `consumers.json(stream: ...): Promise<any>` | `Promise<any>` | promise — `JSON.parse` over the concatenated UTF-8 text |
| `consumers.text(stream: ...): Promise<string>` | `Promise<string>` | promise — UTF-8 decoded |

### Properties & constants

| Name | Type | Default | Notes |
|---|---|---|---|
| `stream.promises` | `object` | — | Same object as `require('node:stream/promises')` (`{ pipeline, finished }`). |
| default `highWaterMark` (byte mode) | `number` | `16384` (16 KiB) | Overridable per-instance (`options.highWaterMark`) or process-wide (`setDefaultHighWaterMark(false, n)`). |
| default `highWaterMark` (object mode) | `number` | `16` (objects) | Overridable per-instance or via `setDefaultHighWaterMark(true, n)`. |

### Events

| Event | Emitted by | Args | Notes |
|---|---|---|---|
| `'close'` | Readable, Writable, Duplex, Transform, PassThrough | `()` | See per-class tables above. |
| `'data'` | Readable (+ Duplex/Transform/PassThrough) | `(chunk)` | Flowing mode only. |
| `'end'` | Readable (+ Duplex/Transform/PassThrough) | `()` | |
| `'error'` | all of the above | `(err: Error)` | |
| `'pause'` | Readable (+ Duplex/Transform/PassThrough) | `()` | |
| `'readable'` | Readable (+ Duplex/Transform/PassThrough) | `()` | |
| `'resume'` | Readable (+ Duplex/Transform/PassThrough) | `()` | |
| `'drain'` | Writable (+ Duplex/Transform/PassThrough) | `()` | |
| `'finish'` | Writable (+ Duplex/Transform/PassThrough) | `()` | |
| `'pipe'` | Writable (+ Duplex/Transform/PassThrough) | `(src: Readable)` | |
| `'unpipe'` | Writable (+ Duplex/Transform/PassThrough) | `(src: Readable)` | |

## 3. Types & option objects

```ts
type BufferEncoding =
  | 'ascii' | 'utf8' | 'utf-8' | 'utf16le' | 'utf-16le' | 'ucs2' | 'ucs-2'
  | 'base64' | 'base64url' | 'latin1' | 'binary' | 'hex';

interface ReadableOptions {
  /** Total bytes (or objects, in object mode) buffered before push() returns false. Default: 16384 / 16. */
  highWaterMark?: number;
  /** Decode chunks to strings with this encoding instead of delivering Buffer. Default: null. */
  encoding?: BufferEncoding | null;
  /** Chunks may be any JS value, not just Buffer/string. Default: false. */
  objectMode?: boolean;
  /** Implementer hook, see §2. */
  read?(this: Readable, size: number): void;
  /** Implementer hook, see §2. */
  construct?(this: Readable, callback: (error?: Error) => void): void;
  /** Implementer hook, see §2. */
  destroy?(this: Readable, error: Error | null, callback: (error?: Error | null) => void): void;
  /** Auto-call destroy() on 'end'/error. Default: true. */
  autoDestroy?: boolean;
  /** Emit 'close' after destroy(). Default: true. */
  emitClose?: boolean;
  /** Node internal/advanced: pre-size the internal buffer array. Rarely used. */
  signal?: AbortSignal;
}

interface WritableOptions {
  /** Default: 16384. */
  highWaterMark?: number;
  /** Default: false. */
  objectMode?: boolean;
  /** Convert string chunks to Buffer before _write. Default: true. */
  decodeStrings?: boolean;
  /** Encoding assumed for string chunks with no explicit encoding. Default: 'utf8'. */
  defaultEncoding?: BufferEncoding;
  /** Implementer hook, see §2. */
  write?(this: Writable, chunk: any, encoding: BufferEncoding, callback: (error?: Error | null) => void): void;
  /** Implementer hook, see §2. */
  writev?(this: Writable, chunks: Array<{ chunk: any; encoding: BufferEncoding }>, callback: (error?: Error | null) => void): void;
  /** Implementer hook, see §2. */
  construct?(this: Writable, callback: (error?: Error) => void): void;
  /** Implementer hook, see §2. */
  destroy?(this: Writable, error: Error | null, callback: (error?: Error | null) => void): void;
  /** Implementer hook, see §2. */
  final?(this: Writable, callback: (error?: Error | null) => void): void;
  autoDestroy?: boolean; // default: true
  emitClose?: boolean;   // default: true
  signal?: AbortSignal;
}

interface DuplexOptions extends ReadableOptions, WritableOptions {
  /** Default: true. false auto-ends the opposite side when one side ends. */
  allowHalfOpen?: boolean;
  /** Override objectMode independently per side; falls back to objectMode. */
  readableObjectMode?: boolean;
  writableObjectMode?: boolean;
  /** Override highWaterMark independently per side; falls back to highWaterMark. */
  readableHighWaterMark?: number;
  writableHighWaterMark?: number;
}

interface TransformOptions extends DuplexOptions {
  /** Required to actually transform (defaults to identity in PassThrough). */
  transform?(this: Transform, chunk: any, encoding: BufferEncoding, callback: (error?: Error | null, data?: any) => void): void;
  flush?(this: Transform, callback: (error?: Error | null, data?: any) => void): void;
}

interface PipelineOptions {
  /** Abort the whole pipeline; every stream is destroyed with the abort reason. */
  signal?: AbortSignal;
  /** Whether the final destination is .end()ed on source completion. Default: true. */
  end?: boolean;
}

interface FinishedOptions {
  /** Reject/callback-error on an 'error' event. Default: true (implied). */
  error?: boolean;
  /** Wait for the readable side specifically (Duplex/Transform). Default: inferred from stream type. */
  readable?: boolean;
  /** Wait for the writable side specifically. Default: inferred from stream type. */
  writable?: boolean;
  signal?: AbortSignal;
  /** Remove finished()'s internal listeners once settled. Default: false (base API) / recommended true. */
  cleanup?: boolean;
}

/** stream/web: UnderlyingSource passed to `new ReadableStream(...)`. */
interface UnderlyingSource<R = any> {
  start?(controller: ReadableStreamDefaultController<R> | ReadableByteStreamController): void | Promise<void>;
  pull?(controller: ReadableStreamDefaultController<R> | ReadableByteStreamController): void | Promise<void>;
  cancel?(reason?: any): void | Promise<void>;
  /** 'bytes' enables BYOB readers + ReadableByteStreamController. */
  type?: 'bytes';
  /** Only meaningful when type === 'bytes'. */
  autoAllocateChunkSize?: number;
}

/** stream/web: UnderlyingSink passed to `new WritableStream(...)`. */
interface UnderlyingSink<W = any> {
  start?(controller: WritableStreamDefaultController): void | Promise<void>;
  write?(chunk: W, controller: WritableStreamDefaultController): void | Promise<void>;
  close?(): void | Promise<void>;
  abort?(reason?: any): void | Promise<void>;
  /** Reserved by the spec; must be undefined. */
  type?: undefined;
}

/** stream/web: Transformer passed to `new TransformStream(...)`. */
interface Transformer<I = any, O = any> {
  start?(controller: TransformStreamDefaultController<O>): void | Promise<void>;
  transform?(chunk: I, controller: TransformStreamDefaultController<O>): void | Promise<void>;
  flush?(controller: TransformStreamDefaultController<O>): void | Promise<void>;
  readableType?: undefined; // reserved
  writableType?: undefined; // reserved
}

/** stream/web: QueuingStrategy passed as the 2nd/3rd ctor arg. */
interface QueuingStrategy<T = any> {
  highWaterMark?: number;
  size?(chunk: T): number;
}

interface StreamPipeOptions {
  preventAbort?: boolean;
  preventCancel?: boolean;
  preventClose?: boolean;
  signal?: AbortSignal;
}

/** Union accepted by stream.pipeline()/stream.compose() stage arguments. */
type Streamable =
  | Readable | Writable | Duplex | Transform
  | Iterable<any> | AsyncIterable<any>
  | ((source?: AsyncIterable<any>, opts?: { signal: AbortSignal }) => AsyncIterable<any> | Iterable<any> | Promise<any>);
```

## 4. Node semantics & edge cases

- **Encodings.** `BufferEncoding` values accepted everywhere a stream decodes/
  encodes strings: `utf8`/`utf-8`, `utf16le`/`utf-16le`, `latin1`/`binary`,
  `base64`, `base64url`, `hex`, `ascii`, `ucs2`/`ucs-2`. `setEncoding()`/
  `defaultEncoding` affect only string *delivery*/*decoding*; internal
  buffering is always byte-accurate.
- **highWaterMark discrepancy after `setEncoding()`.** Once `.setEncoding()`
  converts output to strings, `readable.readableLength`/backpressure
  accounting can drift from the true byte count (a documented Node quirk —
  the "highWaterMark discrepancy" note in the official docs). RTS should
  reproduce this rather than "fix" it, since user code may depend on the
  documented (if awkward) behavior.
- **Platform differences.** `node:stream` itself is platform-agnostic pure
  JS/TS logic — no Windows-vs-POSIX branching, no file descriptors, no errno.
  Platform differences appear only in the *concrete* streams built on top
  (`fs.ReadStream`, `net.Socket`) documented in their own specs.
- **Error codes** (from Node's `errors.md`; exact trigger text should be
  verified against Node source when implementing strict-parity error
  messages — flagged `(verify)` where the fetched doc excerpt did not include
  full description text):
  - `ERR_STREAM_DESTROYED` — `.write()`/`.end()`/`.push()` called on a stream
    already `.destroy()`ed.
  - `ERR_STREAM_WRITE_AFTER_END` — `.write()` called after `.end()`.
  - `ERR_STREAM_PUSH_AFTER_EOF` — `.push()` called after a prior
    `.push(null)`.
  - `ERR_STREAM_UNSHIFT_AFTER_END_EVENT` — `.unshift()` called after `'end'`
    has been emitted.
  - `ERR_STREAM_NULL_VALUES` — `.write(null)` (writing `null` is always
    invalid, even in object mode).
  - `ERR_STREAM_PREMATURE_CLOSE` — used by `finished()`/`pipeline()` when a
    stream's underlying resource closes before the stream logically
    finished/ended (e.g. a socket reset mid-transfer).
  - `ERR_STREAM_ALREADY_FINISHED` `(verify)` — a terminal method (e.g a
    second `.end()`) called after the stream has already finished.
  - `ERR_STREAM_CANNOT_PIPE` `(verify)` / `ERR_STREAM_UNABLE_TO_PIPE`
    `(verify)` — raised in specific `.pipe()` misuse paths (e.g. piping into
    a destination that cannot accept the source's mode); Node has both codes
    historically, confirm which is current in v25.
  - `ERR_STREAM_WRAP` `(verify)` — legacy `.wrap()` interaction error.
  - `ERR_METHOD_NOT_IMPLEMENTED` — `_read`/`_write`/`_transform` invoked
    without ever being overridden by the subclass.
  - `ERR_MULTIPLE_CALLBACK` — an internal completion callback (`_write`'s,
    `_transform`'s, `_final`'s) invoked more than once.
  - `ERR_UNKNOWN_ENCODING` — `.setEncoding()`/`.write(chunk, encoding)`/
    `.setDefaultEncoding()` given an unrecognized encoding string.
  - `ERR_TRANSFORM_ALREADY_TRANSFORMING` — `_flush` invoked while a prior
    `_transform` call's callback has not yet fired.
  - `ERR_TRANSFORM_WITH_LENGTH_0` `(verify)` — a Transform-specific
    zero-length edge case; confirm exact trigger.
  - `ERR_TRAILING_JUNK_AFTER_STREAM_END` `(verify)` — surfaces from
    `zlib`/`DecompressionStream` decoding trailing bytes past a compressed
    stream's logical end; shared with `node:zlib`.
  - `AbortError` (a `DOMException`/Error with `name: 'AbortError'`) — thrown/
    emitted when a `signal` passed to `pipeline()`, `addAbortSignal()`, or a
    WHATWG stream operation aborts.
  - Generic `ERR_INVALID_ARG_TYPE`/`ERR_INVALID_ARG_VALUE`/`ERR_OUT_OF_RANGE`
    apply pervasively to malformed constructor options and method arguments,
    same as elsewhere in Node's API surface.
- **Ordering guarantees.**
  - `write()`'s callback fires strictly before `'finish'` (if this was the
    last queued write before `.end()`), and never fires after `'error'` has
    already been emitted for the same failure.
  - `'drain'` fires only after **all** currently-buffered chunks have been
    accepted by the underlying sink (not just enough to dip below
    `highWaterMark` from one chunk) — the canonical backpressure pattern is
    `if (!stream.write(x)) stream.once('drain', next); else process.nextTick(next);`.
  - For `Transform`/`PassThrough`, `'finish'` (writable side complete) is
    guaranteed to fire *before* `'end'` (readable side complete) only once
    all transformed/flushed output has actually been read out — a slow
    consumer delays `'end'`, not `'finish'`.
  - `pipeline()`/`compose()` always end() every Transform stage even when
    `options.end` is `false` for the *final* destination (Transforms must
    always be logically closed to flush pending output).
- **Backpressure.** `write()` returning `false` is a **hint**, not a hard
  block — Node will still buffer further writes (unbounded, up to available
  memory) rather than throwing; well-behaved producers must voluntarily
  pause. The same discipline applies to `Readable.push()` returning `false`
  inside `_read`.
- **Deprecations.** Streams "classic mode" (pre-v0.10 `'data'`-always-flowing
  API with no `pause()`/pipe backpressure) is fully deprecated — `.wrap()`
  exists specifically to adapt any remaining classic emitters. `stream.Stream`
  itself is legacy but not deprecated (kept as the common base class).
  `readable.push()`/`writable._write()` overriding is the only supported
  implementation pattern; subclassing without implementing the required
  hook throws `ERR_METHOD_NOT_IMPLEMENTED` at first use, not at construction.
- **Security notes.** Streams themselves impose no size limits — an
  attacker-controlled unbounded stream piped without backpressure handling
  (e.g., ignoring `write()`'s `false` return) is a memory-exhaustion vector;
  this is a general "handle backpressure" concern rather than a
  stream-module-specific vulnerability. `pipeline()`/`finished()` are the
  recommended safe patterns specifically because they guarantee cleanup
  (destroying every stage) on error, preventing resource/fd leaks that raw
  `.pipe()` chains can accumulate under error conditions.

## 5. RTS implementation notes

### 5.1 Native impl mapping

Real Node's own `stream` module is itself implemented almost entirely in pure
JS (`lib/internal/streams/*`) over `EventEmitter` — it has essentially **no**
native V8/C++ binding surface of its own. This is the ideal case for RTS's
"no builtins in the engine, `.ts` shim over primordials" doctrine: `Readable`/
`Writable`/`Duplex`/`Transform`/`PassThrough`, `pipeline`/`finished`/
`compose`/`duplexPair`/`isErrored`/`isReadable`/`isWritable`/
`addAbortSignal`/`getDefaultHighWaterMark`/`setDefaultHighWaterMark`, and all
of `stream/promises`/`stream/consumers` are **100% `.ts` state machines**
over already-primordial engine values (`Array`, `Object`, `Function`,
`Promise`, `Buffer`/`Uint8Array`, `Symbol.asyncIterator`) — **no Rust std
module backs this core surface**, exactly like `node:events`' `EventEmitter`.

Concretely:

- `stream.Stream`/`Readable`/`Writable`/`Duplex`/`Transform`/`PassThrough` —
  `.ts` classes; `Readable`/`Writable` each hold an internal buffer (array of
  `{chunk, encoding}` or a single concatenation strategy) plus the documented
  state flags (`_readableState`-analog: `ended`, `flowing`, `buffer`,
  `highWaterMark`, …) as plain object fields. `Duplex` is implemented as a
  `.ts` class that extends `Readable` and copies/mixes in `Writable`'s
  prototype methods (mirrors Node's own `Duplex.prototype = Object.create(
  Readable.prototype)` + `Object.assign(Duplex.prototype, Writable.prototype)`
  pattern) rather than needing multiple engine-level inheritance support.
  `Transform`/`PassThrough` layer `_transform`/`_flush` wiring on top of
  `Duplex`.
- `pipe()`/`unpipe()`/backpressure — `.ts` logic wiring `'data'`/`'drain'`
  listeners between two `.ts` stream instances; no native call.
- `for await (chunk of readable)` / `readable.iterator()` /
  `.map/.filter/.forEach/.toArray/.some/.find/.every/.flatMap/.drop/.take/
  .reduce` — `.ts` async-generator-shaped helpers built on the engine's
  primordial `Promise` + `Symbol.asyncIterator` protocol (same pattern
  `node:events`' `events.on()` async iterator uses).
- `pipeline()`/`finished()`/`compose()`/`duplexPair()`/`isErrored`/
  `isReadable`/`isWritable`/`addAbortSignal`/`getDefaultHighWaterMark`/
  `setDefaultHighWaterMark` — pure `.ts` orchestration over the `.ts` stream
  classes' public event/state surface (no native call needed; `addAbortSignal`
  reuses the already-ambient `AbortSignal`).
- `stream/consumers` (`arrayBuffer`/`blob`/`buffer`/`bytes`/`json`/`text`) —
  `.ts` functions that internally drive the input via `for await` (or
  `pipeline` into an accumulator `Writable`), concatenate chunks with
  `Buffer.concat`/array-push, then produce the requested representation
  (`JSON.parse` for `.json()`, UTF-8 decode for `.text()`, wrap in `Blob` for
  `.blob()`).
- `node:stream/web` (`ReadableStream`/`WritableStream`/`TransformStream`/
  readers/controllers/`ByteLengthQueuingStrategy`/`CountQueuingStrategy`) —
  **do not reimplement in `rts-node`**: these are ambient WHATWG globals that
  must live in `rts-shared`'s Web-standard global infra (same doctrine bucket
  as the existing `EventTarget`/`AbortSignal`/`fetch`/`Response`), because
  they are used well beyond `node:stream` (e.g. `fetch()`'s
  `Response.body`). **This ambient implementation does not exist yet** —
  flagged as the single biggest cross-cutting prerequisite in §5.7.
  `node:stream/web`'s `.ts` shim then does a source-level re-export of those
  ambient identifiers, exactly like `node:events` re-exports the ambient
  `EventTarget`/`Event`/`CustomEvent`.
- `Readable.toWeb`/`fromWeb`, `Writable.toWeb`/`fromWeb`, `Duplex.toWeb`/
  `fromWeb` — `.ts` adapter functions bridging a Node-shaped `.ts` stream
  object to/from a WHATWG-shaped `.ts` stream object; both sides are plain JS
  values, so this is ordinary `.ts` glue, not an ABI concern.
- `TextEncoderStream`/`TextDecoderStream` — thin `.ts` wrappers composing the
  **already-implemented** ambient `TextEncoder`/`TextDecoder` classes (per
  `rts-runtime`'s `globals/text_encoding`) with a `TransformStream`.
- `CompressionStream`/`DecompressionStream` — the **one part of this module
  that genuinely needs a native Rust backend**: real DEFLATE/gzip/Brotli
  codec state (`flate2`, `brotli` crates) cannot be expressed as `.ts`. These
  should be implemented as a thin `.ts` `TransformStream` wrapper around
  native chunked-codec externs **shared with the sibling `node:zlib` module**
  (both need the identical deflate/gzip/brotli context management) — see
  §5.2/§5.7.

### 5.2 ABI surface

Consistent with `node:events`' precedent: **the vast majority of this module
needs zero new `extern "C"` symbols.**

- `NodespaceSpec` entries (mirroring the `fs`/`os`/`process` pattern in
  `crates/rts-node/src/lib.rs`), one per resolvable specifier — the existing
  `ns_prefix_for`/`node_lookup` split on `"node:"` and then look up
  `s.node_module` by exact string, so **submodule paths need no special
  parsing**: registering `node_module: "stream/web"` makes
  `ns_prefix_for("node:stream/web")` resolve for free.

  ```rust
  pub const SPEC: NodespaceSpec = NodespaceSpec {
      node_module: "stream",
      ns_prefix: "node_stream",
      members: &[], // no native members for Readable/Writable/Duplex/Transform/PassThrough/pipeline/finished/…
  };
  pub const SPEC_PROMISES: NodespaceSpec = NodespaceSpec {
      node_module: "stream/promises",
      ns_prefix: "node_stream_promises",
      members: &[],
  };
  pub const SPEC_CONSUMERS: NodespaceSpec = NodespaceSpec {
      node_module: "stream/consumers",
      ns_prefix: "node_stream_consumers",
      members: &[],
  };
  pub const SPEC_WEB: NodespaceSpec = NodespaceSpec {
      node_module: "stream/web",
      ns_prefix: "node_stream_web",
      members: &[], // ReadableStream/etc. are ambient re-exports, not native members
  };
  ```

  An empty `members` slice for `stream`/`stream/promises`/`stream/consumers`/
  `stream/web` is intentional (as with `node:events`): `node_lookup()` never
  needs to resolve anything for these specifiers, while `ns_prefix_for` still
  lets the module loader mount each `.ts` shim under its specifier.

- **The one native member set this module needs**: chunked compression/
  decompression context ops, backing `CompressionStream`/`DecompressionStream`
  (and shared with `node:zlib`'s `Gzip`/`Gunzip`/`Deflate`/`Inflate`/
  `BrotliCompress`/`BrotliDecompress`):

  | Symbol | Args (`AbiType`) | Returns | Notes |
  |---|---|---|---|
  | `__RTS_FN_NODE_STREAM_ZLIB_DEFLATE_INIT` | `I32 format, I32 level` | `Handle` | `format`: 0=deflate,1=deflate-raw,2=gzip. Allocates a `flate2` encoder context, returns a `Handle` into the `rts-node` `HandleTable`/`Entry::Backend`. |
  | `__RTS_FN_NODE_STREAM_ZLIB_DEFLATE_PROCESS` | `Handle ctx, Handle inBuf, U64 inOff, U64 inLen` | `Handle` (output `Buffer`) | Feeds a chunk, returns newly produced compressed bytes as a fresh GC'd buffer handle (may be empty). |
  | `__RTS_FN_NODE_STREAM_ZLIB_DEFLATE_FINISH` | `Handle ctx` | `Handle` (output `Buffer`) | Flushes/finalizes; frees `ctx`. |
  | `__RTS_FN_NODE_STREAM_ZLIB_INFLATE_INIT` | `I32 format` | `Handle` | Symmetric decoder context. |
  | `__RTS_FN_NODE_STREAM_ZLIB_INFLATE_PROCESS` | `Handle ctx, Handle inBuf, U64 inOff, U64 inLen` | `Handle` (output `Buffer`) | |
  | `__RTS_FN_NODE_STREAM_ZLIB_INFLATE_FINISH` | `Handle ctx` | `Handle` (output `Buffer`) | Throws/reports `ERR_TRAILING_JUNK_AFTER_STREAM_END`-equivalent if malformed trailing bytes remain. |
  | `__RTS_FN_NODE_STREAM_ZLIB_BROTLI_COMPRESS_INIT` / `_PROCESS` / `_FINISH` | analogous | analogous | Via the `brotli` crate; mirrors the deflate/gzip trio. |
  | `__RTS_FN_NODE_STREAM_ZLIB_BROTLI_DECOMPRESS_INIT` / `_PROCESS` / `_FINISH` | analogous | analogous | |

  These symbols should physically live under (or be re-exported by) whichever
  module lands first between `node:stream` and `node:zlib` — **recommend
  landing them once, under a shared internal `rts-node` module** (e.g.
  `crates/rts-node/src/zlib_codec/`) that both `stream::web::CompressionStream`
  and `node:zlib`'s classes call into, to avoid duplicating the `flate2`/
  `brotli` context-management code within the same crate.

- **Handles:** `Readable`/`Writable`/`Duplex`/`Transform`/`PassThrough`/
  `ReadableStream`/`WritableStream`/`TransformStream`/readers/controllers are
  **ordinary GC'd JS objects with a shape** — never a `HandleTable` slot
  (there is no Rust-side resource backing them). The **only** `Handle` this
  module introduces is the zlib/Brotli codec context above (an opaque
  `Entry::Backend` payload per §6 of `architecture.md`, since `flate2`/
  `brotli` encoder/decoder state is a real, non-`Copy`, must-be-freed Rust
  struct).

- **`.ts` shim vs native extern split:** ~98% `.ts` shim (all of
  `Readable`/`Writable`/`Duplex`/`Transform`/`PassThrough`/`pipeline`/
  `finished`/`compose`/`duplexPair`/`isErrored`/`isReadable`/`isWritable`/
  `addAbortSignal`/`getDefaultHighWaterMark`/`setDefaultHighWaterMark`/
  `stream/promises`/`stream/consumers`/`stream/web`'s class *shapes*), ~2%
  native extern (the zlib/Brotli codec context ops backing
  `CompressionStream`/`DecompressionStream` only).

### 5.3 Async model

- **Core stream mechanics are synchronous-per-call, deferred via microtask/
  macrotask, never blocking.** `.write()`/`.read()`/`.push()` themselves
  execute synchronously; the *emission* of `'data'`/`'readable'`/`'drain'`/
  `'finish'`/`'end'` is deferred by (real Node's) `process.nextTick`/
  `setImmediate` to avoid "Zalgo" (sometimes-sync-sometimes-async callback
  surprises). RTS's `.ts` implementation must use the engine's microtask
  queue (`queueMicrotask`/`Promise.resolve().then()`) or a `process.nextTick`
  equivalent for the same scheduling — **needs the engine's microtask/event-loop
  drain to actually run** (flagged §5.7, same dependency `node:events`
  already flags).
- **Promise-returning methods** (`readable.toArray()`/`.reduce()`/`.forEach()`/
  etc., `writer.write()`/`.close()`/`.ready`/`.closed`, `stream/promises`'
  `pipeline()`/`finished()`, all of `stream/consumers`) are ordinary `.ts`
  `Promise` machinery — **no tokio needed** for the stream-orchestration
  logic itself. If the underlying concrete stream (`fs.ReadStream`,
  `net.Socket`) is I/O-backed, *that* module's own async model (documented in
  `fs.md`/`net.md`) drives the actual blocking work; `node:stream`'s own
  machinery only reacts to that module's `push()`/`write()` calls.
  callback-form `stream.pipeline(...)`.
- **`CompressionStream`/`DecompressionStream` codec work** is CPU-bound and
  can be non-trivial for large payloads (gzip/Brotli of megabytes). To avoid
  blocking the single-threaded event-loop turn, large `_process` calls
  **should** be dispatched via the shared tokio runtime's `spawn_blocking`
  (matching how `node:crypto`'s hashing/`node:zlib`'s classic API would also
  want to behave) rather than executed synchronously inline for big chunks —
  this needs the shared tokio runtime, currently in `rts-std`'s
  `runtime/async_rt.rs` (flagged §5.7). A first-pass P0 implementation MAY
  run codec `_process` calls synchronously inline (simpler, correct, just not
  maximally concurrent) and defer the `spawn_blocking` optimization.
- **`pipeline()`/WHATWG stream operations with an `AbortSignal`** reuse the
  already-ambient `AbortSignal`/`AbortController` — an abort is a synchronous
  `EventTarget` dispatch on whichever thread owns the signal, not a
  cross-thread wakeup (see §5.4).

### 5.4 Multithread / worker interaction

- A `Readable`/`Writable`/`Duplex`/`Transform`/`PassThrough` (and
  `ReadableStream`/`WritableStream`/`TransformStream`) instance is **ordinary
  per-thread-region heap data** under `docs/specs/rts-threading-model.md` —
  Node itself provides **no API** to hand a live stream object to another
  `worker_threads` thread directly (you cross the thread boundary via
  `MessagePort.postMessage`, which structured-clones/transfers data, never a
  live stream). RTS should preserve exactly this restriction: a stream
  created on one thread stays `threadLocal` by construction.
- **WHATWG streams ARE Transferable** per the Streams Standard + HTML spec —
  `postMessage(readableStream, [readableStream])` can move a `ReadableStream`
  to another realm/thread in browsers and in Node's own `worker_threads`.
  Implementing real cross-thread *transfer* (re-homing the stream's internal
  controller state into the target thread's region, promoting via the shared
  heap per the threading model) is **deferred** (§7) until `node:worker_threads`
  itself maps a `Worker` onto a real RTS thread/region — this module's job is
  only to make the class *shapes* correct single-thread first.
- The zlib/Brotli codec `Handle` (§5.2) wraps non-`Send`-shareable native
  encoder/decoder state; it must **not** be accessible from a thread other
  than the one that created it — attempting to structured-clone/transfer a
  `CompressionStream` whose codec handle lives in another thread's region
  should fail loudly (a documented RTS-specific restriction versus real
  Node/browsers, tracked until real transfer semantics land).

### 5.5 Buffer / TypedArray interop

- **Byte-mode chunks** (`Buffer`/`Uint8Array`/`string`) pushed via
  `readable.push()`/accepted via `writable.write()` are ordinary JS values
  held in a `.ts`-level array/queue — **no ABI marshalling**, since they never
  cross `extern "C"` for the core `Readable`/`Writable`/`Duplex`/`Transform`
  logic (per §5.1/§5.2).
- **WHATWG byte streams (`type: 'bytes'`) + BYOB reader** — `reader.read(view)`
  reads *directly into* the caller-supplied `Buffer`/`TypedArray`/`DataView`
  (zero-copy from the JS perspective: the same backing `ArrayBuffer` is
  written into in place). This is entirely within the engine's primordial
  `ArrayBuffer`/`TypedArray`/`DataView` memory model — again no `extern "C"`
  boundary crossing for the `.ts`-level bookkeeping.
- **`CompressionStream`/`DecompressionStream`** are the one place bytes
  genuinely cross the ABI: each `_process` call passes the input chunk's
  backing `ArrayBuffer` as a `Handle` + `(offset, length)` pair to the native
  codec extern (§5.2), which returns a freshly GC-allocated output `Buffer`
  (also a `Handle`) that the `.ts` wrapper reads back into a `Uint8Array` for
  the `TransformStream`'s `enqueue()`. This mirrors the `fs`/`buffer` modules'
  existing "Handle to backing ArrayBuffer + offset/length" convention (see
  `architecture.md` §9 / `buffer.md`).
- `stream/consumers`' `.arrayBuffer()`/`.buffer()`/`.bytes()` concatenate
  chunks with ordinary `Buffer.concat`/array operations (already `.ts`-level,
  reusing whatever `node:buffer` exposes) — no additional native surface.

### 5.6 Doctrine placement

- `node:stream` (and its three submodules) is **entirely non-primordial**:
  `Readable`/`Writable`/`Duplex`/`Transform`/`PassThrough`/`ReadableStream`/
  `WritableStream`/`TransformStream`/etc. have no native literal/syntactic
  form — the engine must never hardcode any of these class names. (`Buffer`/
  `ArrayBuffer`/`Uint8Array`/`TypedArray` chunks flowing *through* a stream
  remain primordial in their own right, per the existing doctrine — a stream
  is just a JS object that happens to hold references to them.)
- **Resolution path:** `import { Readable } from "node:stream"` → specifier
  stripped of `"node:"` → looked up in `rts-node`'s `NODE_SPECS` table via
  `ns_prefix_for("node:stream")` → `Some("node_stream")`; `"node:stream/web"`
  → `ns_prefix_for` strips `"node:"` leaving `"stream/web"`, matched directly
  against a `NodespaceSpec { node_module: "stream/web", .. }` entry — **no
  change to `ns_prefix_for`'s string-split logic is required**, since it
  already matches the full remaining string against `s.node_module`, and
  `"stream/web"` is just another string. Because every `SPEC*.members` is
  empty (aside from the shared zlib codec ops, which belong to an internal
  helper module, not `stream`'s own `SPEC`), the module resolver's only job
  for `node:stream*` is to mount the right `.ts` file(s) — no `node_lookup()`
  call needs to succeed for the class surface.
- **Native-extern vs `.ts`-shim split:** ~98%/2% per §5.2. `ReadableStream`/
  `WritableStream`/`TransformStream`/`ByteLengthQueuingStrategy`/
  `CountQueuingStrategy`/`TextEncoderStream`/`TextDecoderStream`/
  `CompressionStream`/`DecompressionStream` are **source-level re-exports**
  of ambient globals owned by `rts-shared`'s Web-standard global infra (per
  `architecture.md` §2.2 — "Web-standard global infra (`globals/*`, …)" stays
  in the `rts-std`/`rts-shared` side of the cut line, not `rts-node`) — this
  creates **no Rust-level crate dependency** from `rts-node` onto
  `rts-shared`/`rts-std` (the `.ts` shim references already-in-scope ambient
  identifiers by source, exactly like `node:events`' `EventTarget`
  re-export), honoring "rts-node cannot depend on rts-shared/rts-std" at the
  *crate* level while sharing *class identity* at the *JS-value* level
  (`instanceof ReadableStream` is `true` regardless of whether the object
  came via `node:stream/web` or the bare global).

### 5.7 Shared-infra dependencies (FLAG)

- **Promise/microtask + `process.nextTick`/`setImmediate`-equivalent
  scheduling.** Deferred emission of `'data'`/`'readable'`/`'drain'`/
  `'finish'`/`'end'` (§5.3), and all Promise-returning helpers, need the
  engine's microtask queue to actually drain. This infra currently lives in
  `rts-std` (`runtime/event_loop.rs`/`async_rt.rs`); since `rts-node` cannot
  depend on `rts-std`, it must be reachable from a crate `rts-node` can
  depend on (the `rts-async` hoist target per `architecture.md` §3.2/§7) —
  **identical flag to `node:events`' §5.7**, since both modules are built on
  the same primordial Promise/microtask substrate.
- **Ambient WHATWG Streams Standard globals do not exist yet.**
  `ReadableStream`/`ReadableStreamDefaultReader`/`ReadableStreamBYOBReader`/
  `ReadableStreamDefaultController`/`ReadableByteStreamController`/
  `ReadableStreamBYOBRequest`/`WritableStream`/`WritableStreamDefaultWriter`/
  `WritableStreamDefaultController`/`TransformStream`/
  `TransformStreamDefaultController`/`ByteLengthQueuingStrategy`/
  `CountQueuingStrategy` are **not implemented anywhere** in `rts-shared`'s
  stdlib today (unlike `EventTarget`/`AbortSignal`, which already exist).
  This is the **single biggest cross-cutting prerequisite** this module
  needs — a full WHATWG Streams Standard state machine (locking, reader
  modes, BYOB zero-copy reads, backpressure via `QueuingStrategy.size`,
  teeing, `pipeThrough`/`pipeTo`) must be written in `rts-shared`'s
  Web-global `.ts` infra before `node:stream/web`'s re-export shim can work.
  Flagged here so it is not silently attempted inside `rts-node` (which would
  duplicate a class other modules — `fetch`'s `Response.body`, `Blob.stream()`
  — also need).
- **Existing ambient `TextEncoder`/`TextDecoder`** (`rts-runtime`'s
  `globals/text_encoding`) are a direct dependency for
  `TextEncoderStream`/`TextDecoderStream`.
- **Existing ambient `AbortSignal`/`AbortController`** needed by
  `pipeline()`'s `signal` option, `addAbortSignal()`, and
  `WritableStreamDefaultController.signal`.
- **`flate2`/`brotli` Rust crates** for `CompressionStream`/
  `DecompressionStream` — a **new native dependency for `rts-node`** (owned
  there per `architecture.md` decision 1; duplication versus any `rts-std`
  zlib-adjacent code is accepted). Should be implemented **once**, shared
  in-family with the sibling `node:zlib` module (not a doctrine violation —
  both are `rts-node` modules) rather than twice; sequencing note: whichever
  of `node:stream`/`node:zlib` lands first should own the codec-context
  helper module the other imports.
- **Shared tokio runtime** (`rt().spawn_blocking`) — optional optimization
  for large `CompressionStream`/`DecompressionStream` payloads (§5.3), not
  required for a correct first pass. Currently in `rts-std`'s
  `runtime/async_rt.rs`; same `rts-async` hoist target.
- **No dependency** on `fs`, `net`, `tls`, or `crypto` for this module's own
  logic — the concrete streams those modules return (`fs.ReadStream`,
  `net.Socket`, TLS sockets) merely *extend* `Readable`/`Writable`/`Duplex`
  from here; `node:stream` itself does no I/O.

### 5.8 Implementation phases

(a) Scaffold `NodespaceSpec` entries for `stream`, `stream/promises`,
`stream/consumers`, `stream/web` (all empty `members`, per §5.2), registered
in `NODE_SPECS` in `crates/rts-node/src/lib.rs`.

(b) Port `stream.Stream` (legacy `EventEmitter`-based base) + `Readable` core:
constructor/options, internal buffer + state flags, `push`/`read`/`pause`/
`resume`/`setEncoding`/`destroy`, the `'data'`/`'end'`/`'readable'`/`'pause'`/
`'resume'`/`'close'`/`'error'` events, `_construct`/`_read`/`_destroy` hooks.

(c) Port `Writable` core: constructor/options, `write`/`end`/`cork`/`uncork`/
`setDefaultEncoding`/`destroy`, backpressure (`highWaterMark`/`'drain'`/
`writableNeedDrain`), the `'finish'`/`'pipe'`/`'unpipe'`/`'close'`/`'error'`
events, `_construct`/`_write`/`_writev`/`_final`/`_destroy` hooks.

(d) Port `Duplex` (mixin of (b)+(c), `allowHalfOpen`, per-side
`readableObjectMode`/`writableObjectMode`/`readableHighWaterMark`/
`writableHighWaterMark`) and `Transform`/`PassThrough` (`_transform`/`_flush`
wiring, `ERR_TRANSFORM_ALREADY_TRANSFORMING` guard).

(e) Port `readable.pipe()`/`.unpipe()`/`.wrap()`/`.compose()`, and the
async-iterable helpers (`Symbol.asyncIterator`, `.iterator()`, `.map`/
`.filter`/`.forEach`/`.toArray`/`.some`/`.find`/`.every`/`.flatMap`/`.drop`/
`.take`/`.reduce`) plus `Symbol.asyncDispose` on both `Readable`/`Writable`.

(f) Port `stream.pipeline()`/`stream.finished()` (callback form), `compose()`,
`duplexPair()`, `isErrored`/`isReadable`/`isWritable`, `addAbortSignal()`,
`getDefaultHighWaterMark()`/`setDefaultHighWaterMark()`.

(g) Port `stream/promises` (promise-form `pipeline`/`finished`, wired as
`stream.promises` too) and `stream/consumers` (`arrayBuffer`/`blob`/`buffer`/
`bytes`/`json`/`text`, each internally draining via `for await`).

(h) **Prerequisite patch** (outside `rts-node`, coordinate/flag per §5.7):
land the WHATWG Streams Standard state machine (`ReadableStream`+readers+
controllers, `WritableStream`+writer+controller, `TransformStream`+
controller, `ByteLengthQueuingStrategy`, `CountQueuingStrategy`) as ambient
globals in `rts-shared`'s stdlib.

(i) Re-export those ambient globals from `node:stream/web`'s `.ts` shim; add
`Readable.toWeb`/`fromWeb`, `Writable.toWeb`/`fromWeb`, `Duplex.toWeb`/
`fromWeb` bridging adapters.

(j) Add `TextEncoderStream`/`TextDecoderStream` (thin wrappers over the
existing ambient `TextEncoder`/`TextDecoder`).

(k) Implement the shared zlib/Brotli codec-context native externs (§5.2,
coordinate with `node:zlib`), then wrap as `CompressionStream`/
`DecompressionStream`.

(l) Cross-runtime fixtures (§6); wire into the existing cross-runtime
harness.

## 6. Test plan

`tests/node-stream/*.test.ts` (standard `rts:test` `describe`/`test`/`expect`
template):

1. **Basic `Readable` push/read** — a custom `_read` pushing fixed chunks then
   `push(null)`; assert `'data'` events deliver chunks in order, `'end'`
   fires once, `readableEnded`/`readable` flags flip correctly.
2. **`Readable` paused vs flowing mode** — `.pause()`/`.resume()` toggling;
   assert no `'data'` fires while paused and buffered `readableLength` grows;
   `.read(size)` in paused mode returns exact byte counts.
3. **Basic `Writable` write/end** — a custom `_write` accumulating chunks;
   `.write()` multiple times then `.end()`; assert `'finish'` fires exactly
   once, chunks arrive in order, `.end(chunk, cb)` overload works.
4. **Backpressure** — a slow `_write` (deferred callback); assert `.write()`
   returns `false` once buffered length exceeds `highWaterMark`, `'drain'`
   fires only after the deferred callback resolves, `writableNeedDrain`
   reflects the pending state.
5. **`cork`/`uncork`** — multiple `.write()` calls while corked are batched
   into one `_writev` call; nested `cork()`/`uncork()` calls require matching
   counts.
6. **`Duplex` echo** — a `Duplex` whose `_write` pushes what it received onto
   its own readable side; pipe data in, read the same data out; test
   `allowHalfOpen: false` auto-ending the opposite side.
7. **`Transform` uppercase** — `_transform` calling
   `callback(null, chunk.toString().toUpperCase())`; verify output order and
   correctness; a `_flush` emitting a trailing marker chunk.
8. **`PassThrough`** — data written equals data read, byte-for-byte, in
   object mode and buffer mode.
9. **`pipe()` chaining** — `a.pipe(b).pipe(c)`; assert data flows end-to-end;
   `{end: false}` option leaves `c` open after `a` ends;
   `a.unpipe(b)` stops further delivery mid-stream.
10. **`pipeline()` (callback + promise forms)** — success case (3-stage
    chain, destination fully written); error case (a middle Transform throws)
    — assert every stage is destroyed and the callback/promise reports the
    error exactly once; `AbortSignal` mid-pipeline abort destroys all stages
    with an `AbortError`.
11. **`finished()`** — resolves/callbacks on natural completion; rejects on
    stream error; the returned cleanup function (callback form) actually
    removes listeners (assert no leaked listener count after calling it);
    `{cleanup: true}` promise-form behaves the same automatically.
12. **`addAbortSignal()`** — an already-aborted signal destroys the stream
    immediately with `AbortError`; a signal aborted mid-stream destroys it
    then.
13. **`compose()`/`duplexPair()`** — `compose(readable, transform, writable)`
    behaves like an equivalent `pipeline`; `duplexPair()` — writing to one
    half is readable from the other and vice versa.
14. **`isErrored`/`isReadable`/`isWritable`** — assert correct boolean at each
    lifecycle stage (before/after `destroy()`/`end()`/error).
15. **Async iteration (`for await`)** — consume a `Readable` via `for await`;
    breaking early destroys the source by default; `.iterator({destroyOnReturn:
    false})` does not.
16. **Readable helper methods** — `.map()`/`.filter()`/`.reduce()`/`.toArray()`/
    `.some()`/`.every()`/`.find()`/`.flatMap()`/`.drop()`/`.take()` each on a
    small `Readable.from([1,2,3,4,5])`, asserting correct results and (for
    `.some`/`.every`/`.find`) short-circuit behavior.
17. **Object mode** — a `Readable`/`Writable` pair with `objectMode: true`
    passing plain objects (not `Buffer`/string) through end-to-end; default
    `highWaterMark` of `16` objects verified via `getDefaultHighWaterMark(true)`.
18. **Error propagation** — `.destroy(err)` emits `'error'` then `'close'`
    exactly once each; a second `.destroy()` call is a no-op; writing after
    destroy throws `ERR_STREAM_DESTROYED`; writing `null` throws
    `ERR_STREAM_NULL_VALUES`; `.write()` after `.end()` throws
    `ERR_STREAM_WRITE_AFTER_END`.
19. **`stream/consumers`** — `text()`/`json()`/`buffer()`/`arrayBuffer()`/
    `bytes()`/`blob()` each against a small multi-chunk `Readable.from([...])`
    fixture; `json()` on invalid JSON rejects.
20. **`stream/web` basics** — `new ReadableStream({start, pull, cancel})`
    consumed via `getReader().read()` loop and via `for await`; `.tee()`
    produces two independently-consumable branches; `.pipeThrough()`/
    `.pipeTo()` round-trip through a `TransformStream`.
21. **`stream/web` BYOB reader** — a `type: 'bytes'` `ReadableStream`
    consumed via `getReader({mode: 'byob'})` reading into a caller-supplied
    `Uint8Array`, verifying zero-copy delivery and the `min` option.
22. **`ByteLengthQueuingStrategy`/`CountQueuingStrategy`** — backpressure
    (`desiredSize`) behaves correctly with each strategy on a
    `WritableStream`.
23. **`Readable.toWeb`/`Writable.fromWeb`/`Duplex.toWeb`/`fromWeb`** —
    round-trip a Node `Readable` through `toWeb()` into a WHATWG
    `ReadableStream` and back, verifying byte-identical data.
24. **`TextEncoderStream`/`TextDecoderStream`** — pipe a UTF-8 string through
    an encode→decode `TransformStream` chain, verifying round-trip fidelity
    including multi-byte characters split across chunk boundaries.
25. **`CompressionStream`/`DecompressionStream`** — gzip/deflate/deflate-raw
    round-trip (compress then decompress, compare to original bytes) for
    small and large (>1 MB, multi-chunk) payloads; corrupt-input
    decompression rejects/errors instead of hanging or crashing.
26. **Multithread smoke test (per §5.4)** — create a `Readable` inside a
    `worker_threads` worker, confirm it is **not** visible/usable from the
    parent thread; separately confirm passing raw bytes (not a live stream)
    through a `MessageChannel` between threads still works as the baseline
    this module intentionally does not change.
27. **Adjacent-feature combinations** (testing-creativity mandate) — a
    `Transform` stream used inside a `try/catch` around a throwing
    `_transform`; a `class MyReadable extends Readable` with `super()` and
    an overridden `_read`; a `for await` loop over a `Readable` nested inside
    another loop; `pipeline()` with an async-generator middle stage
    (`async function* (source) { for await (const c of source) yield f(c); }`).

## 7. Open questions / deferrals

- **WHATWG Streams Standard ambient global implementation** — the biggest
  prerequisite (§5.7/§5.8h); not scoped in detail here beyond "must exist
  before `stream/web` can re-export it." Needs its own design pass (locking
  semantics, controller invariants, teeing algorithm, backpressure math) when
  picked up.
- **`CompressionStream` `'brotli'` format** — the Node 25 doc fetch reported
  `'brotli'` as a supported `format` value; the WHATWG Compression Streams
  standard itself only defines `'deflate'`/`'deflate-raw'`/`'gzip'`. Verify
  the exact Node version this Node-specific extension landed in, and confirm
  it is intended to ship for the RTS surface at the same P0 priority as the
  other three formats, or deferred to land alongside `node:zlib`'s own
  Brotli support.
- **Real cross-thread WHATWG stream transfer** (`postMessage(stream,
  [stream])`) — deferred until `node:worker_threads` maps `Worker`/
  `MessagePort` onto real RTS threads/regions per the threading model (§5.4).
- **Exact error-code trigger conditions** for `ERR_STREAM_ALREADY_FINISHED`,
  `ERR_STREAM_CANNOT_PIPE`/`ERR_STREAM_UNABLE_TO_PIPE`, `ERR_STREAM_WRAP`,
  `ERR_TRANSFORM_WITH_LENGTH_0`, `ERR_TRAILING_JUNK_AFTER_STREAM_END` — the
  fetched Node docs excerpt gave code names without full description text;
  verify each against Node source (`lib/internal/streams/*`,
  `lib/internal/errors.js`) before shipping strict-parity error messages.
- **`highWaterMark` discrepancy after `setEncoding()`** (§4) — decide whether
  RTS reproduces this exact Node quirk (byte-accounting drift once string
  decoding is enabled) or documents a deliberate, explicit deviation; default
  recommendation is to reproduce it for parity, since some real-world code
  may rely on the documented (if awkward) numbers.
- **`process.nextTick`-equivalent scheduling granularity** — real Node
  interleaves `process.nextTick` (higher priority) and `setImmediate`/
  microtask-queue draining in specific documented order; whether RTS's
  microtask queue (once the `rts-async` hoist lands) needs a *distinct*
  nextTick-priority lane, or whether ordinary microtasks are a sufficient
  approximation for `node:stream`'s internal scheduling, is an open question
  to resolve when the async infra hoist (§5.7) is designed.
- **`fs.ReadStream`/`WriteStream`, `net.Socket`, `zlib.Gzip`/`Deflate`,
  `crypto.Cipher`/`Hash`, `child_process` stdio** all extend classes defined
  here — this spec intentionally does not re-document their module-specific
  behavior; see each module's own `docs/node-implementation/*.md`.
