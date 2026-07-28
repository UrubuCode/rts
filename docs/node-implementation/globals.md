# Node.js Globals

**RTS engine spec — Node.js 25 parity.**

> **Placement (binding):** the global surface **belongs to the engine**, not to
> `rts-node`. `rts-node` mirrors only the `node:` *module* API and adds **no new
> globals**. Every identifier below is **engine-surfaced**; its implementation is
> sourced from the layer that owns the type (primordial → engine/`rts-primitives`;
> Web-standard class → `rts-shared`; backend singleton like `process`/`console` →
> `rts-node`/`rts-std`, but the global *binding* is the engine's). See
> [`layering.md`](./layering.md) §3. §2.0 below classifies each global accordingly.

| Field | Value |
|---|---|
| Module | *(none)* — `globals` is not an importable specifier. This spec documents every identifier Node.js injects into `globalThis` (or, for CJS, into file-local scope) **without any `import`/`require`** — `https://nodejs.org/docs/latest-v25.x/api/globals.html`. |
| Node.js version | 25.x |
| Stability | Mixed — see the per-member "Since / Stability" column in §2.0. Ranges from **3 - Legacy** (`global`, `atob`/`btoa`) through **1.2 - Release candidate** (`localStorage`/`sessionStorage`) and **1 - Experimental** (`navigator.locks`, `URLPattern`) to **2 - Stable** (the majority: `fetch`, `URL`, `AbortController`, `crypto`, `console`, `process`, …). |
| Tier | P0 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | **None, by definition** — every entry in §2 is reachable with zero `import`/`require` in every module (CJS and ESM alike). A few also have an explicit `node:*` module form that re-exports the *same* object/class (`node:buffer` → `Buffer`/`Blob`/`File`/`atob`/`btoa`; `node:console` → `console`; `node:process` → `process`; `node:timers` → the `setTimeout` family; `node:events` → `EventTarget`/`Event`/`CustomEvent`; `node:worker_threads` → `MessageChannel`/`MessagePort`/`BroadcastChannel`; `node:perf_hooks` → `performance`; `node:crypto` → `crypto`/`Crypto`/`SubtleCrypto`; `node:module` → the CJS wrapper params) — those forms are documented in their own module spec, cross-referenced throughout. |
| Globals exposed | `globalThis`, `global`, `__dirname`, `__filename`, `exports`, `module`, `require`, `console`, `process`, `Buffer`, `Blob`, `File`, `atob`, `btoa`, `AbortController`, `AbortSignal`, `TextEncoder`, `TextDecoder`, `TextEncoderStream`, `TextDecoderStream`, `URL`, `URLSearchParams`, `URLPattern`, `fetch`, `Request`, `Response`, `Headers`, `FormData`, `structuredClone`, `queueMicrotask`, `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`, `setImmediate`, `clearImmediate`, `Event`, `EventTarget`, `CustomEvent`, `MessageEvent`, `CloseEvent`, `ErrorEvent`, `MessageChannel`, `MessagePort`, `BroadcastChannel`, `performance`, `crypto`, `Crypto`, `CryptoKey`, `SubtleCrypto`, `DOMException`, `WebAssembly`, `ReadableStream` (+ controllers/readers), `WritableStream` (+ controller/writer), `TransformStream`, `ByteLengthQueuingStrategy`, `CountQueuingStrategy`, `CompressionStream`, `DecompressionStream`, `navigator`, `localStorage`, `sessionStorage`, `Storage`, `WebSocket`, `EventSource`. |

## 1. Purpose

This document is **not** a `node:<module>` spec — it is the map of Node's
**ambient global object surface**: everything a Node.js (and therefore RTS)
program can reference with zero `import`/`require`, injected into the realm
before user code runs. It exists so that no single global identifier falls
through the cracks between the ~40 per-module specs in this directory (many of
which *also* re-export one or more of these same ambient classes — `buffer.md`
covers `Buffer`/`Blob`/`File`/`atob`/`btoa`, `events.md` covers
`Event`/`EventTarget`/`CustomEvent`, `worker_threads.md` covers
`MessageChannel`/`MessagePort`/`BroadcastChannel`, `crypto.md` covers the
`crypto` Web Crypto singleton, `perf_hooks.md` covers `performance`,
`process.md`/`console.md`/`module.md`/`timers.md` cover their own singletons).
This spec's unique job is **§2.0**: a single classification table stating, for
every global, whether it is engine-primordial, part of the shared Web-standard
global infrastructure ("rts-std web-global" in the architecture doc's
language — today split across `rts-shared`'s native `globals/*` +
`rts-shared/src/stdlib/*.ts` + `rts-std`'s native `globals/*`), or a
Node-specific `rts-node` concern — plus **full first-party detail** for the
handful of globals (`AbortController`/`AbortSignal`, `URL`/`URLSearchParams`/
`URLPattern`, `fetch`/`Request`/`Response`/`Headers`/`FormData`,
`structuredClone`, `queueMicrotask`, `DOMException`, `navigator`/
`localStorage`/`sessionStorage`, `WebAssembly`, `WebSocket`/`EventSource`) that
have **no other dedicated spec doc** in this directory.

## 2. Exported API surface (COMPLETE)

### 2.0 Classification table (the connective tissue this doc uniquely provides)

| Global | Kind | Owner tier | Today's home (as of this writing) | Canonical spec | Since |
|---|---|---|---|---|---|
| `globalThis` | object (realm global) | **engine-core** — the ECMA-262 realm global object accessor; not Node-specific, not Web-specific | `rts-shared/src/globals/global_this` (native `GlobalClassSpec`) | this doc §2.20 | ES2020 |
| `global` | object (alias of `globalThis`) | **rts-node concern** — Node-only legacy identifier, no browser equivalent | not yet implemented (thin `.ts` alias planned) | this doc §2.20 | v0.1.27, **3 - Legacy** |
| `__dirname` | string (CJS file-scope) | rts-node | `crates/rts-node` (planned, CJS module wrapper) | `module.md` | N/A |
| `__filename` | string (CJS file-scope) | rts-node | `crates/rts-node` (planned) | `module.md` | N/A |
| `exports` | object (CJS file-scope) | rts-node | `crates/rts-node` (planned) | `module.md` | N/A |
| `module` | object (CJS file-scope) | rts-node | `crates/rts-node` (planned) | `module.md` | N/A |
| `require()` | function (CJS file-scope) | rts-node | `crates/rts-node` (planned) | `module.md` | N/A |
| `console` | object (singleton) | rts-node (module identity) backed by shared infra | `rts-std/src/globals/console` (native) + `rts-shared/src/stdlib/console.ts` (ambient prelude) — dual today, see `console.md` §5.6 migration note | `console.md` | v0.1.100 |
| `process` | object (singleton, `EventEmitter`) | rts-node | not yet implemented (`crates/rts-node`, planned) | `process.md` | v0.1.7 |
| `Buffer` | class (extends `Uint8Array`) | rts-node | not yet implemented (planned) | `buffer.md` | v0.1.103 |
| `Blob` | class | rts-node module identity / rts-std web-global backing | `rts-std/src/globals/blob` (native) + `rts-shared/src/stdlib/webapi.ts` (`.ts` value holder) — **overlap flagged in §5.8** | `buffer.md` | v18.0.0 |
| `File` | class (extends `Blob`) | rts-node module identity / rts-std web-global backing | same as `Blob` | `buffer.md` | v20.0.0 |
| `atob()` | function | rts-node | `rts-std/src/globals/text_encoding` (native, currently bundled with `TextEncoder`) | `buffer.md` | v16.0.0, **3 - Legacy** |
| `btoa()` | function | rts-node | same as `atob` | `buffer.md` | v16.0.0, **3 - Legacy** |
| `AbortController` | class | rts-std web-global | `rts-shared/src/stdlib/events.ts` (pure `.ts`, **already implemented**) | this doc §2.6 | v15.0.0/14.17.0 |
| `AbortSignal` | class (extends `EventTarget`) | rts-std web-global | `rts-shared/src/stdlib/events.ts` (pure `.ts`, **already implemented**) | this doc §2.6 | v15.0.0/14.17.0 |
| `TextEncoder` | class | rts-std web-global | `rts-std/src/globals/text_encoding` (native, **already implemented**) | this doc §2.8 | v11.0.0 |
| `TextDecoder` | class | rts-std web-global | `rts-std/src/globals/text_encoding` (native, **already implemented**) | this doc §2.8 | v11.0.0 |
| `TextEncoderStream` | class | rts-std web-global | `rts-shared/src/stdlib/streams.ts` (**already implemented**) | `stream.md` | v18.0.0 |
| `TextDecoderStream` | class | rts-std web-global | `rts-shared/src/stdlib/streams.ts` (**already implemented**) | `stream.md` | v18.0.0 |
| `URL` | class | rts-std web-global | `rts-shared/src/globals/url` (native, **already implemented**) | this doc §2.9 | v10.0.0 |
| `URLSearchParams` | class | rts-std web-global | `rts-shared/src/globals/url` (native, **already implemented**) | this doc §2.9 | v10.0.0 |
| `URLPattern` | class | rts-std web-global | not yet implemented | this doc §2.9 | v23.8.0/22.13.1, **1 - Experimental** |
| `fetch()` | function | rts-std web-global | `rts-std/src/globals/fetch` (native, **already implemented**) | this doc §2.10 | v17.5.0/16.15.0 |
| `Request` | class | rts-std web-global | `rts-std/src/globals/fetch` (native) + `rts-shared/src/stdlib/webapi.ts` (`.ts` value holder) — **overlap flagged in §5.8** | this doc §2.10 | v17.5.0/16.15.0 |
| `Response` | class | rts-std web-global | `rts-std/src/globals/fetch` (native) + `rts-shared/src/stdlib/webapi.ts` — same overlap | this doc §2.10 | v17.5.0/16.15.0 |
| `Headers` | class | rts-std web-global | `rts-std/src/globals/headers` (native) + `rts-shared/src/stdlib/webapi.ts` — same overlap | this doc §2.10 | v17.5.0/16.15.0 |
| `FormData` | class | rts-std web-global | `rts-std/src/globals/form_data` (native) + `rts-shared/src/stdlib/webapi.ts` — same overlap | this doc §2.10 | v17.6.0/16.15.0 |
| `structuredClone()` | function | rts-std web-global | `rts-shared/src/stdlib/structured_clone.ts` (pure `.ts`, **already implemented**, Map/Set/Date/cycle-aware) | this doc §2.11 | v17.0.0 |
| `queueMicrotask()` | function | rts-std / rts-async shared infra | `rts-shared/src/stdlib/timers.ts` (`.ts` surface) over `rts-std/src/event_loop.rs` (native drain) | this doc §2.12 | v11.0.0 |
| `setTimeout`/`clearTimeout`/`setInterval`/`clearInterval`/`setImmediate`/`clearImmediate` | functions + `Timeout`/`Immediate` classes | rts-std / rts-async shared infra | `rts-shared/src/stdlib/timers.ts` + `rts-std/src/globals/timers` (native) | `timers.md` | v0.0.1 |
| `Event` | class | rts-std web-global | `rts-shared/src/stdlib/events.ts` (**already implemented**) | `events.md` | v15.0.0 |
| `EventTarget` | class | rts-std web-global | `rts-shared/src/stdlib/events.ts` (**already implemented**) | `events.md` | v15.0.0 |
| `CustomEvent` | class (extends `Event`) | rts-std web-global | **not yet implemented** — flagged missing by `events.md` §5.7, this doc §5.7 | `events.md` | v18.7.0/16.17.0 |
| `MessageEvent` | class (extends `Event`) | rts-std web-global | not yet implemented as a standalone class (worker_threads shims a duck-typed literal, see `worker_threads.md`) | this doc §2.5 / `worker_threads.md` | v15.0.0 |
| `CloseEvent` | class (extends `Event`) | rts-std web-global | not yet implemented | this doc §2.5 | v22.4.0 *(verify)* |
| `ErrorEvent` | class (extends `Event`) | rts-std web-global | not yet implemented | this doc §2.5 | v22.4.0 *(verify)* |
| `MessageChannel` | class | rts-std web-global | `rts-shared/src/stdlib/events.ts` (**already implemented**, same-thread `queueMicrotask` delivery only) | `worker_threads.md` | v15.0.0 |
| `MessagePort` | class (extends `EventTarget`) | rts-std web-global | `rts-shared/src/stdlib/events.ts` (**already implemented**, same-thread only) | `worker_threads.md` | v15.0.0 |
| `BroadcastChannel` | class (extends `EventTarget`) | rts-std web-global | **not yet implemented** — flagged in this doc's §5.7/§5.8 | `worker_threads.md` | v15.4.0 |
| `performance` | object (singleton) | rts-std/rts-shared web-global | `rts-shared/src/stdlib/performance.ts` (`now()`/`timeOrigin` only today) | `perf_hooks.md` | v16.0.0 |
| `crypto` | object (singleton, Web Crypto) | rts-std web-global | not yet a dedicated ambient global export today (native crypto primitives exist in `rts-std/src/crypto`; the Web Crypto `Crypto`/`SubtleCrypto` object wrapper is the work item) | `crypto.md` | v17.6.0/16.15.0 |
| `Crypto` | class | rts-std web-global | see `crypto` row | `crypto.md` | v19.0.0 (global, no flag) |
| `CryptoKey` | class | rts-std web-global | see `crypto` row | `crypto.md` | v15.0.0 |
| `SubtleCrypto` | class | rts-std web-global | see `crypto` row | `crypto.md` | v17.6.0/16.15.0 |
| `DOMException` | class | rts-std web-global | `rts-shared/src/stdlib/domexception.ts` (**already implemented**) | this doc §2.16 | v17.0.0 (global) |
| `WebAssembly` | namespace object | **deferred / engine-adjacent** — see §5.6, §7 (no V8; needs an RTS-native WASM engine) | not implemented | this doc §2.15 | v8.0.0 |
| `ReadableStream` + controllers/readers | classes | rts-std web-global | `rts-shared/src/stdlib/streams.ts` (**already implemented**) | `stream.md` | v18.0.0 |
| `WritableStream` + controller/writer | classes | rts-std web-global | `rts-shared/src/stdlib/streams.ts` (**already implemented**) | `stream.md` | v18.0.0 |
| `TransformStream` | class | rts-std web-global | `rts-shared/src/stdlib/streams.ts` (**already implemented**) | `stream.md` | v18.0.0 |
| `ByteLengthQueuingStrategy` / `CountQueuingStrategy` | classes | rts-std web-global | `rts-shared/src/stdlib/streams.ts` | `stream.md` | v18.0.0 |
| `CompressionStream` / `DecompressionStream` | classes | rts-std web-global | not yet implemented (needs `flate2`/brotli, see `stream.md`) | `stream.md` | v17.0.0/18.0.0 |
| `navigator` | object (singleton) | rts-std web-global, **deferred** | not implemented | this doc §2.18 | v21.0.0, **1 - Experimental** |
| `localStorage` / `sessionStorage` | objects (singleton) | rts-std web-global, **deferred** | not implemented | this doc §2.18 | v22.4.0, **1.2 - Release candidate** |
| `Storage` | class (interface) | rts-std web-global, **deferred** | not implemented | this doc §2.18 | v22.4.0 |
| `WebSocket` | class | rts-std web-global, **deferred** | not implemented | this doc §2.19 | v21.0.0 / stable v22.4.0 |
| `EventSource` | class | rts-std web-global, **deferred** | not implemented | this doc §2.19 | v22.3.0, **1 - Experimental** |

### 2.1 CJS module-scope pseudo-globals — `__dirname`, `__filename`, `exports`, `module`, `require()`

**Not real `globalThis` properties** — file-scope parameters injected by the
CJS module wrapper. **Fully specified in `module.md`** (§2 "Module Wrapper");
this doc only records their presence in the ambient surface for completeness.
No content is duplicated here — see `module.md` for the wrapper signature,
`require.cache`/`require.resolve`/`require.extensions` surface, and
`Module.wrap()`.

### 2.2 `console` and `process` singletons

Both are **rts-node** concerns for their explicit `node:console`/`node:process`
module identity (custom `Console` instances, the full `process.*` surface);
the *default ambient instance* each singleton exposes is backed by shared
Web-standard/runtime infrastructure that predates the `rts-node` rewrite (see
`console.md` §5.1/§5.6, `process.md`). **Fully specified in `console.md` and
`process.md`** — not duplicated here.

### 2.3 `Buffer`, `Blob`, `File`, `atob`, `btoa`

All five are documented **exhaustively in `buffer.md`**. Summary for this
doc's completeness requirement only:

| Global | One-line role |
|---|---|
| `Buffer` | Node's original binary-data class, `extends Uint8Array`; construction only via static factories (`Buffer.from`/`.alloc`/…). |
| `Blob` | Immutable, chunked byte container with a MIME `type`; `size`/`text()`/`arrayBuffer()`/`slice()`/`stream()`. |
| `File` | `extends Blob`, adds `name`/`lastModified`. |
| `atob(data)` | Legacy base64 → binary string decode. **3 - Legacy**, prefer `Buffer.from(data, 'base64')`. |
| `btoa(data)` | Legacy binary string → base64 encode. **3 - Legacy**, prefer `buf.toString('base64')`. |

### 2.4 Timer family — `setTimeout`/`setInterval`/`setImmediate` + `clear*` + `Timeout`/`Immediate`

Documented **exhaustively in `timers.md`** (which also covers `node:timers` and
`node:timers/promises`). Summary: six ambient functions plus the two opaque
handle-returning classes `Timeout` (`setTimeout`/`setInterval`) and
`Immediate` (`setImmediate`), each with `ref()`/`unref()`/`hasRef()`/
`[Symbol.dispose]()` (`Timeout` additionally has `refresh()`/`close()`/
`[Symbol.toPrimitive]()`). Not re-derived here.

### 2.5 `Event`, `EventTarget`, `CustomEvent`, `MessageEvent`, `CloseEvent`, `ErrorEvent`

`Event`/`EventTarget`/`CustomEvent` are documented **exhaustively in
`events.md`** (§2, "class EventTarget"/"class CustomEvent"). Not re-derived
here except to note the three sibling `Event` subclasses `events.md` does
**not** cover (no dedicated Node doc references them outside `worker_threads.md`'s
tangential `MessageEvent` usage) — full detail below since this is their only
home in this directory.

#### `class MessageEvent extends Event`

**Constructor**: `new MessageEvent(type: string, init?: MessageEventInit)`.

| Property | Type | Notes |
|---|---|---|
| `event.data` | `any` | The delivered payload. |
| `event.origin` | `string` | Empty string outside a browser-style cross-origin context (Node/RTS always same-origin). |
| `event.lastEventId` | `string` | Empty string unless set via `init`. |
| `event.source` | `MessagePort \| null` | Sending port, when applicable. |
| `event.ports` | `readonly MessagePort[]` | Transferred ports, if any. |

No instance methods beyond inherited `Event` members. Dispatched today (per
`worker_threads.md`) as a **duck-typed literal** (`{ data }`), not a real
`MessageEvent` instance — upgrading `MessagePort.postMessage`/`BroadcastChannel`
delivery to construct a real `MessageEvent` is a `§5.8` task shared with
`events.md`/`worker_threads.md`.

#### `class CloseEvent extends Event` *(verify exact Node-added version)*

Used by the `WebSocket` global's `'close'` event.

| Property | Type | Notes |
|---|---|---|
| `event.code` | `number` | WebSocket close status code. |
| `event.reason` | `string` | Close reason string. |
| `event.wasClean` | `boolean` | `true` if the closing handshake completed cleanly. |

**Constructor**: `new CloseEvent(type: string, init?: CloseEventInit)`.

#### `class ErrorEvent extends Event` *(verify exact Node-added version)*

Used by the `WebSocket`/`EventSource` globals' `'error'` events.

| Property | Type | Notes |
|---|---|---|
| `event.message` | `string` | Human-readable error message. |
| `event.error` | `any` | The underlying `Error` object, if any. |
| `event.filename` | `string` | Empty in RTS (no browser script-URL concept). |
| `event.lineno` / `event.colno` | `number` | `0` in RTS. |

**Constructor**: `new ErrorEvent(type: string, init?: ErrorEventInit)`.

### 2.6 `AbortController` / `AbortSignal`

**No dedicated module doc exists for these** — full detail here (the current
`rts-shared/src/stdlib/events.ts` implementation, read during research for
this spec, already matches the shape below almost exactly).

#### `class AbortController`

Base class: none. Not an `EventEmitter`/`EventTarget` itself (its `.signal` is).

**Constructor**: `new AbortController()` — no arguments.

| Member | Signature | Notes |
|---|---|---|
| `controller.signal` | `AbortSignal` (readonly property) | Created at construction; never changes identity. |
| `controller.abort(reason?)` | `abort(reason?: any): void` | Idempotent — a second call is a no-op. Flips `.signal.aborted`, sets `.signal.reason` (defaults to `new DOMException("This operation was aborted", "AbortError")` when `reason` is omitted), then synchronously fires `'abort'` on `.signal`. |

#### `class AbortSignal extends EventTarget`

No public constructor — obtained via `controller.signal`, or the three static
factories below.

**Static methods**

| Signature | Params | Returns | Since |
|---|---|---|---|
| `AbortSignal.abort(reason?)` | `reason?: any` | `AbortSignal`, already `aborted === true` | v15.12.0/14.17.0 |
| `AbortSignal.timeout(delay)` | `delay: number` (ms) | `AbortSignal` that aborts after `delay` ms with `reason = new DOMException("The operation timed out.", "TimeoutError")` | v17.3.0/16.14.0 |
| `AbortSignal.any(signals)` | `signals: Iterable<AbortSignal>` | Composite `AbortSignal`; aborts (propagating the triggering signal's `reason`) as soon as **any** input signal aborts — immediately if one is already aborted | v20.3.0/18.17.0 |

**Instance properties**

| Property | Type | Notes |
|---|---|---|
| `signal.aborted` | `boolean` (readonly) | `false` until abort. |
| `signal.reason` | `any` (readonly) | `undefined` until abort; set exactly once. |
| `signal.onabort` | `((event: Event) => void) \| null` | Property-style handler, in addition to `addEventListener('abort', …)`. |

**Instance methods**

| Signature | Returns | Throws |
|---|---|---|
| `signal.throwIfAborted()` | `void` | Throws `signal.reason` (rethrows the exact value, not wrapped) iff `aborted`. |

**Events**: `'abort'` — dispatched **exactly once**, synchronously, from
inside `controller.abort()` or the static factory's internal trigger (a fired
timer for `.timeout()`, the first aborting source for `.any()`).

### 2.7 `MessageChannel` / `MessagePort` / `BroadcastChannel`

`MessageChannel` and `MessagePort` are documented in depth in
`worker_threads.md` (§2 "class MessageChannel"/"class MessagePort", §3 types,
§5.4 threading-model mapping) since their primary use case — and their only
*real* cross-thread semantics — is `worker_threads`. Summary for this doc:

| Class | Base | Constructor | Key members |
|---|---|---|---|
| `MessageChannel` | none | `new MessageChannel()` | `.port1`, `.port2` (paired `MessagePort`s, `.port1.__peer === port2`) |
| `MessagePort` | `EventTarget` | none (obtained via `MessageChannel`/`Worker.parentPort`) | `postMessage(value, transferList?)`, `start()`, `close()`, `ref()`/`unref()`/`hasRef()`, `.onmessage` |

`BroadcastChannel extends EventTarget` is **not yet implemented anywhere in
the tree** (verified — no `BroadcastChannel` identifier exists in
`crates/rts-shared` or `crates/rts-std` as of this writing), despite
`worker_threads.md` describing its target shape (`worker_threads.md` §2 "class
BroadcastChannel"). This doc flags the gap in §5.7/§5.8; the class shape
(`new BroadcastChannel(name)`, `postMessage(message)`, `close()`, `ref()`/
`unref()`, `.onmessage`/`.onmessageerror`, events `'message'`/`'messageerror'`)
is fully specified in `worker_threads.md` §2 and not repeated here.

### 2.8 `TextEncoder` / `TextDecoder` (+ `*Stream` siblings)

#### `class TextEncoder`

**Constructor**: `new TextEncoder()` — always UTF-8, no arguments (per spec —
unlike `TextDecoder`, `TextEncoder` has no encoding parameter).

| Member | Signature | Notes |
|---|---|---|
| `encoder.encoding` | `"utf-8"` (readonly) | Always this literal value. |
| `encoder.encode(input?)` | `encode(input?: string): Uint8Array` | UTF-8 bytes; `input` defaults to `""`. |
| `encoder.encodeInto(src, dest)` | `encodeInto(src: string, dest: Uint8Array): { read: number, written: number }` | Encodes into a caller-provided buffer without allocating; `read` = UTF-16 code units consumed, `written` = bytes produced. |

#### `class TextDecoder`

**Constructor**: `new TextDecoder(encoding?: string, options?: TextDecoderOptions)`
— `encoding` defaults to `"utf-8"`; supports the full WHATWG Encoding Standard
label set (`utf-8`, `utf-16le`, `utf-16be`, `windows-1252`, `iso-8859-1`,
`shift_jis`, `gbk`, `euc-jp`, … — Node's ICU-backed implementation supports the
whole label registry when built with `full-icu`; RTS should document exactly
which subset it supports, see §4).

| Member | Signature | Notes |
|---|---|---|
| `decoder.encoding` | `string` (readonly) | Normalized label. |
| `decoder.fatal` | `boolean` (readonly) | From `options.fatal` (default `false`) — throw `TypeError` on malformed input instead of substituting U+FFFD. |
| `decoder.ignoreBOM` | `boolean` (readonly) | From `options.ignoreBOM` (default `false`). |
| `decoder.decode(input?, options?)` | `decode(input?: ArrayBuffer \| ArrayBufferView, options?: { stream?: boolean }): string` | `options.stream: true` retains incomplete multi-byte sequences across calls (streaming decode). |

`TextEncoderStream`/`TextDecoderStream` (the `TransformStream`-shaped
streaming siblings) are documented in `stream.md` (`node:stream/web`) — not
repeated here.

### 2.9 `URL`, `URLSearchParams`, `URLPattern`

#### `class URL`

**Constructor**: `new URL(input: string, base?: string | URL)` — throws
`TypeError` (`"Invalid URL"`) if the input, resolved against `base` when
given, fails the WHATWG URL parser.

**Static methods**

| Signature | Returns | Since |
|---|---|---|
| `URL.canParse(input, base?)` | `boolean` | v19.9.0/18.17.0 |
| `URL.parse(input, base?)` | `URL \| null` (never throws) | v22.1.0 |

**Instance properties** (all are accessor pairs — get + set — except
`origin`/`searchParams`, which are get-only)

| Property | Type | Notes |
|---|---|---|
| `url.href` | `string` | The full serialized URL; setting reparses (may throw `TypeError`). |
| `url.origin` | `string` (readonly) | `protocol://host` (or `"null"` for opaque origins). |
| `url.protocol` | `string` | Includes trailing `:`. |
| `url.username` / `url.password` | `string` | Percent-encoded. |
| `url.host` | `string` | `hostname` + `:port` when port is non-default. |
| `url.hostname` | `string` | No port; IPv6 without brackets when read raw internally but bracketed in `href`. |
| `url.port` | `string` | Empty string for the scheme's default port. |
| `url.pathname` | `string` | |
| `url.search` | `string` | Includes leading `?` when non-empty. |
| `url.searchParams` | `URLSearchParams` (readonly) | Live view — mutating it updates `url.search` and vice versa. |
| `url.hash` | `string` | Includes leading `#` when non-empty. |

**Instance methods**: `url.toString(): string` (alias of `.href`),
`url.toJSON(): string` (alias of `.href`, used by `JSON.stringify`).

#### `class URLSearchParams`

**Constructor overloads**: `new URLSearchParams()`; `new URLSearchParams(init: string)`
(leading `?` optional); `new URLSearchParams(init: Record<string,string>)`;
`new URLSearchParams(init: Iterable<[string,string]>)`.

| Member | Signature | Notes |
|---|---|---|
| `usp.size` | `number` (readonly) | v19.8.0/18.16.0. |
| `usp.append(name, value)` | `void` | Always adds a new entry. |
| `usp.delete(name, value?)` | `void` | `value` filter added v20.2.0/18.18.0 (delete only matching pairs). |
| `usp.get(name)` | `string \| null` | First matching value. |
| `usp.getAll(name)` | `string[]` | All matching values, in insertion order. |
| `usp.has(name, value?)` | `boolean` | `value` filter added alongside `delete`'s. |
| `usp.set(name, value)` | `void` | Replaces first match, removes the rest, or appends. |
| `usp.sort()` | `void` | Stable sort by name (UTF-16 code unit order). |
| `usp.toString()` | `string` | `application/x-www-form-urlencoded` serialization. |
| `usp.forEach(cb, thisArg?)` | `void` | |
| `usp.entries()` / `.keys()` / `.values()` / `[Symbol.iterator]()` | iterators | `entries()` and the default iterator yield `[name, value]`. |

#### `class URLPattern` *(Experimental, v23.8.0/22.13.1 — verify exact version against the v25.x build)*

**Constructor**: `new URLPattern(input?: string | URLPatternInit, baseURL?: string, options?: URLPatternOptions)`.

| Member | Signature | Notes |
|---|---|---|
| `pattern.test(input, baseURL?)` | `(input: string \| URLPatternInit, baseURL?: string) => boolean` | |
| `pattern.exec(input, baseURL?)` | `(input: string \| URLPatternInit, baseURL?: string) => URLPatternResult \| null` | Per-component named capture groups. |
| `pattern.protocol` / `.username` / `.password` / `.hostname` / `.port` / `.pathname` / `.search` / `.hash` | `string` (readonly) | Compiled pattern strings per component. |
| `pattern.hasRegExpGroups` | `boolean` (readonly) | `true` if any component uses a custom regex group. |

### 2.10 `fetch`, `Request`, `Response`, `Headers`, `FormData`

#### `fetch(input, init?): Promise<Response>`

| Param | Type | Optional? | Default |
|---|---|---|---|
| `input` | `string \| URL \| Request` | no | — |
| `init` | `RequestInit` | yes | `{}` |

Returns: `Promise<Response>`. Rejects with `TypeError` on a malformed URL,
network failure, or an aborted `init.signal`. Variant: **promise**.

Node-specific notes (must hold for RTS parity, §4): built on `undici` in real
Node; a custom `dispatcher` may be passed via `init.dispatcher` (or globally
via `setGlobalDispatcher()`), which RTS does not need to reproduce literally
(no `undici` dependency — RTS's own HTTP client backs it, see §5.1) but should
offer an equivalent extension point. `mode`/`credentials`/`cache` request
options are accepted for API compatibility but are largely no-ops outside a
browser (no cookie jar, no CORS) — document this explicitly rather than
silently ignoring, per §4.

#### `class Headers`

**Constructor**: `new Headers(init?: HeadersInit)` — `HeadersInit` = `Headers
| [string, string][] | Record<string, string>`.

| Member | Signature | Notes |
|---|---|---|
| `h.append(name, value)` | `void` | Adds without removing existing entries of the same name. |
| `h.delete(name)` | `void` | |
| `h.get(name)` | `string \| null` | Multiple same-name values are joined with `", "` (except `set-cookie`). |
| `h.getSetCookie()` | `string[]` | One entry per `Set-Cookie` header, never combined. |
| `h.has(name)` | `boolean` | |
| `h.set(name, value)` | `void` | Replaces all entries of that name. |
| `h.forEach(cb, thisArg?)` | `void` | |
| `h.entries()` / `.keys()` / `.values()` / `[Symbol.iterator]()` | iterators | Iteration order: unique names sorted lexicographically (`set-cookie` yields one entry per value, per fetch spec). |

Names are case-insensitively stored (lower-cased internally); comparisons and
iteration keys are always lower-case.

#### `class FormData`

**Constructor**: `new FormData()` — no arguments (Node does not implement the
browser's `HTMLFormElement`-sourced constructor overload).

| Member | Signature | Notes |
|---|---|---|
| `fd.append(name, value, filename?)` | `void` | `filename` only meaningful when `value` is Blob/File-like. |
| `fd.delete(name)` | `void` | |
| `fd.get(name)` | `FormDataEntryValue \| null` | |
| `fd.getAll(name)` | `FormDataEntryValue[]` | |
| `fd.has(name)` | `boolean` | |
| `fd.set(name, value, filename?)` | `void` | Replaces first match, drops the rest, or appends. |
| `fd.forEach(cb, thisArg?)` | `void` | |
| `fd.entries()` / `.keys()` / `.values()` / `[Symbol.iterator]()` | iterators | Insertion order (no name sorting, unlike `Headers`). |

#### `class Request`

**Constructor**: `new Request(input: string | URL | Request, init?: RequestInit)`.

| Property | Type | Notes |
|---|---|---|
| `req.method` | `string` (readonly) | Upper-cased; default `"GET"`. |
| `req.url` | `string` (readonly) | |
| `req.headers` | `Headers` (readonly) | |
| `req.redirect` | `"follow" \| "error" \| "manual"` (readonly) | Default `"follow"`. |
| `req.signal` | `AbortSignal` (readonly) | Never `null` — a never-aborting signal when `init.signal` omitted. |
| `req.body` | `ReadableStream<Uint8Array> \| null` (readonly) | |
| `req.bodyUsed` | `boolean` (readonly) | |
| `req.cache` / `.credentials` / `.destination` / `.integrity` / `.keepalive` / `.mode` / `.referrer` / `.referrerPolicy` | various (readonly) | Present for API compatibility; several are non-operative outside a browser (§4). |

**Methods**: `req.clone(): Request`; body consumers `arrayBuffer()`,
`blob()`, `bytes()` *(newer addition, verify version)*, `formData()`,
`json()`, `text()` — all `(): Promise<T>`, each throws `TypeError` if
`bodyUsed` is already `true` or a second consumer races the first.

#### `class Response`

**Constructor**: `new Response(body?: BodyInit | null, init?: ResponseInit)`.

**Static methods**

| Signature | Returns |
|---|---|
| `Response.error()` | A `Response` whose `type` is `"error"` (network-error sentinel) |
| `Response.json(data, init?)` | `Response` with `Content-Type: application/json` and `JSON.stringify(data)` as body |
| `Response.redirect(url, status?)` | Redirect `Response` (`status` defaults to `302`) |

| Property | Type | Notes |
|---|---|---|
| `res.status` | `number` (readonly) | Default `200`. |
| `res.statusText` | `string` (readonly) | |
| `res.ok` | `boolean` (readonly) | `status` in `[200, 300)`. |
| `res.headers` | `Headers` (readonly) | |
| `res.redirected` | `boolean` (readonly) | |
| `res.type` | `"basic" \| "cors" \| "error" \| "opaque" \| "opaqueredirect"` (readonly) | Always `"basic"` outside true cross-origin semantics. |
| `res.url` | `string` (readonly) | |
| `res.body` | `ReadableStream<Uint8Array> \| null` (readonly) | |
| `res.bodyUsed` | `boolean` (readonly) | |

**Methods**: `res.clone(): Response`; the same body-consumer methods as
`Request` (`arrayBuffer`/`blob`/`bytes`/`formData`/`json`/`text`, all
`Promise`-returning).

### 2.11 `structuredClone`

##### `structuredClone(value, options?): any`

| Param | Type | Optional? | Default |
|---|---|---|---|
| `value` | `any` | no | — |
| `options` | `{ transfer?: any[] }` | yes | `{}` |

Returns: a deep, structural clone. Preserves reference identity for cycles and
shared sub-objects (`clone.self === clone` when `value.self === value`).
Throws (spec-mandated) `DOMException` (`name: "DataCloneError"`) on a value
that cannot be cloned (functions, most host objects) or on an `options.transfer`
entry that is not transferable. Variant: **sync**.

### 2.12 `queueMicrotask`

##### `queueMicrotask(callback): void`

| Param | Type | Optional? |
|---|---|---|
| `callback` | `() => void` | no |

Returns: `void`. Schedules `callback` to run as a microtask: after the
currently executing synchronous code and after `process.nextTick`'s queue
fully drains, but before the event loop proceeds to the next phase/macrotask.
An exception thrown inside `callback` is **not** caught by the scheduler — it
surfaces as an `'uncaughtException'` on `process` (real Node behavior; RTS
must match, not swallow it). Variant: **callback (microtask-scheduled)**.

### 2.13 `performance`

Documented **exhaustively in `perf_hooks.md`**. The ambient global is
`globalThis.performance` — the *same object identity* as
`require('node:perf_hooks').performance` (Node guarantee RTS must preserve).
Not re-derived here.

### 2.14 `crypto` (Web Crypto global)

Documented **exhaustively in `crypto.md`** (§2 "Crypto (Web Crypto global)",
"SubtleCrypto"). One classification note this doc adds: the ambient `crypto`
global (`Crypto`/`CryptoKey`/`SubtleCrypto`) is architecturally **distinct**
from `node:crypto`'s extended Node-only surface (`createHash`, `createCipheriv`,
`randomBytes`, X.509 helpers, …) even though `crypto.md` documents both under
one file because Node itself does so (`node:crypto`'s default export *is* the
extended object, and `crypto.webcrypto`/`globalThis.crypto` is a sub-property
of it). Per `architecture.md` §13 open decision 3: `node:crypto`'s extended
surface is an **rts-node** concern with its **own** hash/CSPRNG implementation;
the ambient `crypto` Web Crypto global is an **rts-std web-global** backed by
`rts-std/src/crypto`'s primitives — accepted duplication, not a shared crate.

### 2.15 `WebAssembly`

Namespace object exposing `WebAssembly.compile`/`.instantiate`/`.validate`/
`Module`/`Instance`/`Memory`/`Table`/`Global`/`CompileError`/`LinkError`/
`RuntimeError` per the W3C WebAssembly JS API. **No implementation exists
anywhere in the RTS tree.** Per `architecture.md` §11's binding "no V8"
doctrine, this cannot be satisfied by embedding a WASM engine's V8 integration
— it needs either (a) an RTS-native WASM interpreter/AOT compiler (a
significant, currently unscoped engineering effort — Cranelift itself has a
WASM front-end, `cranelift-wasm`, which is the most plausible starting point
since RTS already depends on Cranelift), or (b) a documented deferral. This
spec **defers** `WebAssembly` — see §7.

### 2.16 `DOMException`

**Constructor**: `new DOMException(message?: string, name?: string)` — both
default to `""`/`"Error"` respectively (real Node/browsers default `name` to
`"Error"`, not one of the legacy names below).

| Member | Type | Notes |
|---|---|---|
| `ex.message` | `string` (readonly) | |
| `ex.name` | `string` (readonly) | |
| `ex.code` | `number` (readonly, getter) | The **legacy numeric code** for the 22 spec-listed `name`s (`IndexSizeError`=1, …, `DataCloneError`=25); `0` for any other `name` (including `AbortError`/`TimeoutError`, which have no legacy code). |

**Static + instance legacy code constants** (spec-mandated, both
`DOMException.INDEX_SIZE_ERR` and `err.INDEX_SIZE_ERR` must resolve):
`INDEX_SIZE_ERR=1`, `DOMSTRING_SIZE_ERR=2`, `HIERARCHY_REQUEST_ERR=3`,
`WRONG_DOCUMENT_ERR=4`, `INVALID_CHARACTER_ERR=5`, `NO_DATA_ALLOWED_ERR=6`,
`NO_MODIFICATION_ALLOWED_ERR=7`, `NOT_FOUND_ERR=8`, `NOT_SUPPORTED_ERR=9`,
`INUSE_ATTRIBUTE_ERR=10`, `INVALID_STATE_ERR=11`, `SYNTAX_ERR=12`,
`INVALID_MODIFICATION_ERR=13`, `NAMESPACE_ERR=14`, `INVALID_ACCESS_ERR=15`,
`VALIDATION_ERR=16`, `TYPE_MISMATCH_ERR=17`, `SECURITY_ERR=18`,
`NETWORK_ERR=19`, `ABORT_ERR=20`, `URL_MISMATCH_ERR=21`,
`QUOTA_EXCEEDED_ERR=22`, `TIMEOUT_ERR=23`, `INVALID_NODE_TYPE_ERR=24`,
`DATA_CLONE_ERR=25`. Node v26.0.0 adds a `QuotaExceededError` subclass
(`DOMException` becomes its parent) — out of scope for Node 25 parity but
worth flagging for the next version bump.

### 2.17 Streams — `ReadableStream`/`WritableStream`/`TransformStream` + friends

Documented **exhaustively in `stream.md`** (`node:stream/web` section):
`ReadableStream` (+ `ReadableStreamDefaultController`,
`ReadableStreamDefaultReader`, `ReadableStreamBYOBReader`,
`ReadableStreamBYOBRequest`), `WritableStream` (+
`WritableStreamDefaultController`, `WritableStreamDefaultWriter`),
`TransformStream`, `ByteLengthQueuingStrategy`, `CountQueuingStrategy`,
`CompressionStream`/`DecompressionStream`, `TextEncoderStream`/
`TextDecoderStream`. Not re-derived here.

### 2.18 `navigator`, `localStorage`, `sessionStorage`, `Storage` — deferred, low priority

| Global | Members | Stability | Notes |
|---|---|---|---|
| `navigator` | `.hardwareConcurrency: number`, `.language: string`, `.languages: string[]`, `.platform: string`, `.userAgent: string`, `.locks: LockManager` (v24.5.0+, Experimental) | **1 - Experimental** (disable with `--no-experimental-global-navigator`) | A read-only informational object; `.locks` overlaps with `worker_threads.md`'s `Lock`/`LockManager` (same classes, cross-referenced there). |
| `localStorage` | `Storage` instance | **1.2 - Release candidate** (disable with `--no-experimental-webstorage`) | Persisted to disk via `--localstorage-file`; shared across all callers in the process; 10 MB quota (`QuotaExceededError` on overflow). |
| `sessionStorage` | `Storage` instance | **1.2 - Release candidate** | In-memory only, per-process; same 10 MB quota. |
| `Storage` (class) | `getItem(key)`, `setItem(key, value)`, `removeItem(key)`, `clear()`, `key(index)`, `.length` (readonly) | — | Standard Web Storage interface; both globals implement it. |

Not a P0 priority (RTS is not a browser and has no natural "origin" concept
for storage partitioning) — see §7 for the deferral rationale.

### 2.19 `WebSocket`, `EventSource` — deferred, low priority

| Global | Role | Stability | Notes |
|---|---|---|---|
| `WebSocket` | Browser-compatible WebSocket client | **2 - Stable** (since v22.4.0; Experimental v21–v22.3) | Node's is `undici`-backed; `http.md` §4 already notes the WebSocket-upgrade handshake in passing. RTS needs its own client over `rts-std`'s `net`/`tls` — real work, deferred pending a P1 pass. |
| `EventSource` | Server-Sent Events client | **1 - Experimental** (v22.3.0+) | Also `undici`-backed in Node. Deferred alongside `WebSocket`. |

## 3. Types & option objects

```ts
// --- AbortController / AbortSignal ---
type AbortReason = any; // typically a DOMException, but any value is accepted

// --- URL / URLPattern ---
interface URLPatternInit {
  protocol?: string; username?: string; password?: string;
  hostname?: string; port?: string; pathname?: string;
  search?: string; hash?: string; baseURL?: string;
}
interface URLPatternOptions { ignoreCase?: boolean; }
interface URLPatternComponentResult {
  input: string;
  groups: Record<string, string | undefined>;
}
interface URLPatternResult {
  inputs: [string] | [URLPatternInit, string];
  protocol: URLPatternComponentResult; username: URLPatternComponentResult;
  password: URLPatternComponentResult; hostname: URLPatternComponentResult;
  port: URLPatternComponentResult; pathname: URLPatternComponentResult;
  search: URLPatternComponentResult; hash: URLPatternComponentResult;
}

// --- fetch / Request / Response / Headers / FormData ---
type HeadersInit = Headers | [string, string][] | Record<string, string>;
type BodyInit =
  | string | Blob | BufferSource | FormData | URLSearchParams
  | ReadableStream<Uint8Array>;
type FormDataEntryValue = string | Blob; // File extends Blob

interface RequestInit {
  method?: string;
  headers?: HeadersInit;
  body?: BodyInit | null;
  redirect?: "follow" | "error" | "manual";
  signal?: AbortSignal | null;
  // Node/undici extension, no browser meaning required for RTS parity:
  dispatcher?: unknown;
  // Present for API compatibility; largely no-ops outside a browser (§4):
  cache?: string; credentials?: string; integrity?: string;
  keepalive?: boolean; mode?: string; referrer?: string; referrerPolicy?: string;
}
interface ResponseInit {
  status?: number;      // default 200
  statusText?: string;  // default ""
  headers?: HeadersInit;
}

// --- structuredClone ---
interface StructuredCloneOptions { transfer?: Transferable[]; }
type Transferable = ArrayBuffer /* | MessagePort, in a future increment */;

// --- TextDecoder ---
interface TextDecoderOptions { fatal?: boolean; ignoreBOM?: boolean; }
interface TextDecodeOptions { stream?: boolean; }
interface TextEncoderEncodeIntoResult { read: number; written: number; }

// --- Event / CustomEvent / MessageEvent / CloseEvent / ErrorEvent ---
interface EventInit { bubbles?: boolean; cancelable?: boolean; composed?: boolean; }
interface CustomEventInit<T = any> extends EventInit { detail?: T; }
interface MessageEventInit<T = any> extends EventInit {
  data?: T; origin?: string; lastEventId?: string;
  source?: MessagePort | null; ports?: MessagePort[];
}
interface CloseEventInit extends EventInit {
  code?: number; reason?: string; wasClean?: boolean;
}
interface ErrorEventInit extends EventInit {
  message?: string; filename?: string; lineno?: number; colno?: number; error?: any;
}
interface AddEventListenerOptions {
  once?: boolean; passive?: boolean; capture?: boolean; signal?: AbortSignal;
}
type EventListenerLike = ((event: Event) => void) | { handleEvent(event: Event): void };

// --- Storage ---
interface Storage {
  readonly length: number;
  key(index: number): string | null;
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
  clear(): void;
}
```

## 4. Node semantics & edge cases

- **`global` vs `globalThis`.** In Node, top-level `var`/function declarations
  in a CJS file are module-scoped, **not** attached to `global`/`globalThis`,
  regardless of module type — a very common misconception. RTS must not leak
  top-level `let`/`const`/`function` bindings onto the ambient global object
  (project memory already records a past bug class here: a `let` at module
  top-level accidentally shadowing/clobbering an unrelated prelude gcell).
- **`atob`/`btoa` input validation.** `atob()` throws `DOMException`
  (`InvalidCharacterError`, legacy code 5) on a string containing characters
  outside the base64 alphabet; it does **not** silently ignore them (unlike
  some older non-Node implementations).
- **`structuredClone` unsupported types.** Functions, `WeakMap`/`WeakSet`
  (by spec — not cloneable), and (in RTS specifically, until §7's typed-array/
  Map/Set/Date coverage lands further) any backend-opaque Registry-class
  instance with no enumerable own keys degrade to an as-is reference return
  rather than throwing `DataCloneError` — an intentional, documented interim
  behavior difference from the spec (see `structured_clone.ts`'s own comment),
  not silent data loss for the common JSON-shaped case.
- **`queueMicrotask` ordering vs `process.nextTick`.** `process.nextTick`
  callbacks always drain **before** the microtask queue (`queueMicrotask`/
  resolved-`Promise` continuations) on each turn — a Node-specific ordering
  quirk inherited from before Promises existed; RTS's event loop must
  replicate this two-queue priority, not treat them as one queue.
- **`fetch` and non-browser semantics.** No cookie jar, no CORS enforcement,
  no `Origin` header injection by default; `mode: "cors"`/`credentials:
  "include"` are accepted but not meaningfully enforced (matches real Node's
  own "Node.js does not implement the full WHATWG CORS model" caveat) — this
  must be **documented as intentional**, not silently divergent behavior.
- **`fetch` default `Content-Type`/body handling** — a `string` body defaults
  to `text/plain;charset=UTF-8`; a `URLSearchParams` body defaults to
  `application/x-www-form-urlencoded;charset=UTF-8`; a `FormData` body
  produces a `multipart/form-data; boundary=...` encoding RTS's native fetch
  implementation must generate correctly (arbitrary boundary generation +
  correct part framing) for interop with real HTTP servers.
- **`TextDecoder` encoding-label coverage.** Real Node uses ICU (`full-icu`)
  for the complete WHATWG label registry (`shift_jis`, `gbk`, `iso-2022-jp`,
  …); RTS has no ICU dependency today. Document explicitly which labels RTS
  supports (`utf-8` mandatory; recommend also `utf-16le`/`utf-16be`/
  `windows-1252`/`iso-8859-1` as a pragmatic subset) rather than silently
  accepting an unsupported label and mis-decoding.
- **`AbortSignal.timeout()` and the event loop.** A timer-armed `AbortSignal`
  keeps the timer (and therefore, absent `.unref()`-equivalent semantics, the
  event loop) alive until it fires — same "does this keep the process alive"
  question `timers.md` §4 covers for `setTimeout`.
- **`MessagePort`/`MessageChannel` today are same-thread only** (`events.ts`
  delivers via `queueMicrotask`) — this is **not yet a real cross-thread
  primitive**; do not assume `worker_threads.md`'s cross-thread guarantees
  hold before that module's own implementation phases land (see this doc's
  §5.4 and `worker_threads.md` §5.1).
- **`DOMException` is not `instanceof Error`** per the WHATWG spec (a
  historical wart — it predates the `Error` hierarchy) even though it has
  `message`/`name`/`stack`-like fields and is commonly used like one; RTS's
  `.ts` implementation (§2.16) matches this by *not* extending `Error`. A
  `try { … } catch (e) { if (e instanceof Error) }` guard around abort/timeout
  code must therefore **not** assume it will catch a `DOMException`.
- **Windows vs POSIX** — nothing in this doc's surface is directly
  platform-sensitive (unlike `fs`/`os`/`path`), except transitively through
  `fetch`'s networking stack (TLS root store resolution, `AF_INET6`
  availability) — covered in `net.md`/`tls.md`, not here.
- **Security note — `eval`-adjacent surfaces.** None of this doc's globals
  execute arbitrary code (`WebAssembly.compile` is the closest, and it is
  deferred/unimplemented). `structuredClone` does not invoke user-defined
  `toJSON`/getters the way `JSON.stringify` does — it walks own enumerable
  properties directly, which is a meaningful behavioral difference to
  document, not a bug.

## 5. RTS implementation notes

### 5.1 Native impl mapping

This doc's surface splits cleanly into two ownership clusters:

**(A) rts-node-owned pieces** (Node-specific identity/ergonomics): `Buffer`,
`console`'s `Console` class, `process`, the CJS `module`/`exports`/`require`/
`__dirname`/`__filename` wrapper. Native Rust mapping is **fully specified in
each module's own §5.1**: `buffer.md`, `console.md`, `process.md`, `module.md`.
Not re-derived here.

**(B) rts-std/rts-shared-owned pieces** (the majority — the shared
Web-standard global infrastructure `architecture.md` §2 explicitly keeps in
`rts-std`, not moving to `rts-node`):

| Cluster | Native backing | Crate/file |
|---|---|---|
| `URL`/`URLSearchParams` | A WHATWG-compliant URL parser (already hand-written; percent-encoding, IDNA-lite host handling) | `rts-shared/src/globals/url/instance.rs` |
| `TextEncoder`/`TextDecoder`/`atob`/`btoa` | UTF-8 encode/decode via Rust `std` (`String::from_utf8`, `.as_bytes()`); base64 codec shared with `crypto`'s | `rts-std/src/globals/text_encoding/instance.rs` |
| `fetch`/`Response`/`Request` (native cluster) | RTS's own HTTP/1.1 client over `std::net` + `rustls` (**not** `undici`, per the no-embedded-JS-engine-dependency posture — RTS reimplements, never links a JS-authored HTTP stack) | `rts-std/src/globals/fetch/instance.rs`, reusing `rts-std/src/net`, `rts-std/src/tls` |
| `crypto` (Web Crypto) | SHA-256 (FIPS 180-4, already inline), CSPRNG (`BCryptGenRandom`/`/dev/urandom`, already implemented); **needs extending** for `SubtleCrypto`'s AES/RSA/ECDSA/HMAC/HKDF/PBKDF2 surface (`crypto.md`'s job) | `rts-std/src/crypto/mod.rs` |
| `Event`/`EventTarget`/`CustomEvent`/`AbortController`/`AbortSignal`/`MessageChannel`/`MessagePort` | **Pure `.ts`** — parallel-array listener storage, synchronous dispatch, no native code at all | `rts-shared/src/stdlib/events.ts` |
| `Headers`/`FormData`/`Blob`/`File`/`Request`/`Response` (`.ts` value-holder cluster) | **Pure `.ts`** — insertion-ordered parallel arrays, UTF-8 helpers hand-rolled over primordial `string`/`Array` ops | `rts-shared/src/stdlib/webapi.ts` |
| `structuredClone` | **Pure `.ts`** — recursive clone with a seen/clone memo for cycles; `Map`/`Set`/`Date` type-preserving via `instanceof`; falls back to `engine.buffer_clone`/`engine.buffer_detach` bridges only for `ArrayBuffer` byte-level clone/transfer | `rts-shared/src/stdlib/structured_clone.ts` |
| `DOMException` | **Pure `.ts`** — name/message/legacy-code table | `rts-shared/src/stdlib/domexception.ts` |
| `queueMicrotask`, `setTimeout` family | `.ts` surface (`timers.ts`) over the native microtask/timer-wheel drain | `rts-shared/src/stdlib/timers.ts` + `rts-std/src/event_loop.rs` + `rts-std/src/globals/timers` |
| `performance` | `.ts` singleton (`now()`/`timeOrigin` only today; `perf_hooks.md` extends it) | `rts-shared/src/stdlib/performance.ts` |
| Streams (`ReadableStream`/etc.) | **Pure `.ts`** state machine (per-controller queue + backpressure signaling) | `rts-shared/src/stdlib/streams.ts` |
| `BroadcastChannel`, `CustomEvent`, `URLPattern`, `WebAssembly`, `navigator`/`localStorage`/`sessionStorage`, `WebSocket`/`EventSource` | **Not yet implemented** anywhere | — (§5.8, §7) |

The pure-`.ts` cluster is the doctrinal ideal (§5.6) — it needs **zero**
native extern surface because every operation is expressible over primordials
(`Array`, `Function`, `Promise`, `string`, plain objects) the engine already
lowers. The native cluster exists only where real OS/network/crypto/ICU work
is unavoidable.

### 5.2 ABI surface

Two pre-existing, **distinct** symbol conventions already in use for this
surface (neither is the `__RTS_FN_NODE_<MODULE>_<NAME>` convention this
directory's `rts-node` specs use — that convention is reserved for
**rts-node**-owned modules only):

- **`__RTS_FN_GL_<CLASS>_<NAME>`** — the existing "global class" convention
  (`abi::global_class.rs`/`GLOBAL_CLASS_SPECS`, per `CLAUDE.md`'s ABI section)
  for the native (B)-cluster classes: `__RTS_FN_GL_URL_*`,
  `__RTS_FN_GL_USP_*` (URLSearchParams), `__RTS_FN_GL_TEXTENC_*`/
  `__RTS_FN_GL_TEXTDEC_*`, `__RTS_FN_GL_FETCH*`, `__RTS_FN_GL_FETCH_RESPONSE_*`,
  `__RTS_FN_GL_REQUEST_*`. Rich instances (`URL`, in-flight `fetch` requests)
  are opaque `Handle`s (`__RTS_FN_GL_URL_FREE` confirms lifecycle-managed
  handles already exist for `URL`).
- **Zero externs** for the pure-`.ts` cluster (`Event`/`EventTarget`/
  `AbortController`/`AbortSignal`/`MessageChannel`/`MessagePort`/`Headers`/
  `FormData`/`Blob`/`File`/`structuredClone`/`DOMException`/streams/
  `performance`'s `now()`) — these are ordinary GC'd JS objects with a shape
  the engine's hidden-class model already handles; no `HandleTable` slot is
  needed since there is no Rust-side resource behind them. This mirrors
  `events.md` §5.2's finding for `node:events` exactly.
- **`crypto` (Web Crypto)** — will need new `__RTS_FN_GL_CRYPTO_*`/
  `__RTS_FN_GL_SUBTLE_*` externs as `crypto.md`'s SubtleCrypto surface lands;
  `CryptoKey` instances are opaque `Handle`s (key material must never be a
  plain inspectable JS value).

### 5.3 Async model

| Area | Model | Notes |
|---|---|---|
| `Event`/`EventTarget` dispatch | **sync** | Listeners run in registration order on the calling stack, per spec — no event-loop hop. |
| `AbortController.abort()` | **sync** | Fires `'abort'` synchronously inside the call. |
| `AbortSignal.timeout(ms)` | **timer-based** | Arms a real `setTimeout`; needs the timer/event-loop primitive. |
| `queueMicrotask` | **microtask** | Needs the microtask queue to actually drain — see §5.7. |
| `structuredClone` | **sync** | No I/O, no scheduling. |
| `fetch()` | **promise**, backed by real async I/O | Needs the shared tokio runtime + TLS/net (§5.7) — this is the one genuinely blocking-I/O-bound area in this doc's surface. |
| `crypto.subtle.*` (all `SubtleCrypto` methods) | **promise, mandatory** even for trivially fast operations (spec requirement, matches `crypto.md`'s finding) | Does not necessarily need `spawn_blocking`/tokio for CPU-bound hashing, but must still go through the Promise settle machinery for API shape. |
| `MessagePort.postMessage` (today) | **microtask** (`queueMicrotask`) | Same-thread only; will become a cross-thread channel enqueue once `worker_threads` lands real threads (§5.4). |
| `Timeout`/`Immediate`/`setTimeout` family | **timer/event-loop** | Fully specified in `timers.md` §5.3. |

### 5.4 Multithread / worker interaction

- **The overwhelming majority of this doc's classes are ordinary
  per-thread-region heap data** under `docs/specs/rts-threading-model.md` —
  `Event`/`EventTarget`/`AbortController`/`AbortSignal`/`Headers`/`FormData`/
  `Blob`/`File`/`Request`/`Response`/`URL`/`URLSearchParams`/`DOMException`
  instances are `threadLocal` by construction; nothing in this doc's surface
  requires `shared`/promotion-on-publication semantics on its own (same
  finding as `events.md` §5.4).
- **`MessageChannel`/`MessagePort`/`BroadcastChannel` are the one real
  cross-thread-shaped exception** — per `architecture.md` §8 and
  `worker_threads.md` §5.4, `MessagePort`/`postMessage` is the concrete
  user-facing embodiment of the threading model's promotion-on-publication
  write barrier: a `postMessage`d value must be structured-cloned into the
  target thread's region (or, for `SharedArrayBuffer`, promoted to the shared
  heap), never handed over by raw reference. Today's `.ts` implementation
  (`queueMicrotask`-based, same-thread) is a correctness placeholder — it does
  **not yet** cross a real OS thread, and must not be assumed to until
  `worker_threads` maps a `Worker` onto a real RTS thread/region.
- **`crypto.getRandomValues`/CSPRNG** must remain safe to call concurrently
  from multiple threads without contending on one global mutex — the existing
  `rts-std/src/crypto` CSPRNG already uses the OS entropy source per-call
  (`BCryptGenRandom`/`/dev/urandom`), which is inherently thread-safe; no
  shared mutable RNG state to guard (unlike `math.random`'s xorshift, which
  does have per-call state — not part of this doc's surface).
- **`fetch()`'s underlying HTTP client** shares the same connection-pooling/
  tokio-runtime thread-safety story as `node:http`'s eventual implementation —
  see `http.md`/`net.md` for the detailed mapping; this doc only needs to flag
  the dependency (§5.7).

### 5.5 Buffer / TypedArray interop

- `TextEncoder.encode()`/`.encodeInto()` produce/consume `Uint8Array` — the
  primordial TypedArray, no extra marshalling beyond what any `Uint8Array`
  already gets.
- `Blob`/`File` (both the native and `.ts` clusters) carry byte payloads;
  `.arrayBuffer()` must return a real `ArrayBuffer`, `.stream()` a
  `ReadableStream<Uint8Array>` yielding chunks — the `.ts` `Blob.stream()`
  today enqueues exactly one UTF-8-encoded chunk (§5.1's `webapi.ts` excerpt).
- `Request`/`Response.body` is a `ReadableStream<Uint8Array>` — every chunk
  crossing from the native fetch client into the `.ts` stream must be a
  `Uint8Array` view over a GC-tracked buffer, not a raw pointer.
- `crypto.getRandomValues(typedArray)` fills a **caller-provided** typed array
  in place and returns the same reference (per spec — this is one of the few
  Web APIs that mutates its argument rather than returning a new value); the
  native extern must write directly into the passed buffer's backing memory
  (bounds-checked, max 65,536 bytes per `crypto.md`'s cap) rather than
  allocating and copying.
- `SubtleCrypto.digest`/`.encrypt`/`.decrypt`/`.sign`/etc. return
  `Promise<ArrayBuffer>` (spec-mandated — never a `Buffer`/`Uint8Array`
  directly), consistent with `crypto.md`'s own finding.
- `structuredClone`'s `options.transfer` list, when it contains an
  `ArrayBuffer`, detaches the **source** (`byteLength` reads `0` afterward)
  after copying its bytes into the clone — implemented today via the
  `engine.buffer_clone`/`engine.buffer_detach` private bridges (§5.1).

### 5.6 Doctrine placement

- **Every class/function in this doc is non-primordial** except `globalThis`
  itself, which is a core ECMA-262 realm concept the engine's base runtime
  already provides (not Web-standard, not Node-specific) — the engine must
  never hardcode any of `URL`/`fetch`/`Headers`/`AbortController`/`Event`/
  `crypto`/`structuredClone`/… by name in `crates/rts-codegen-new/`.
- **This surface is *not* resolved via the `node:` scheme prefix at all** —
  unlike every other doc in this directory, none of §2's globals (except the
  handful with an explicit `node:*` re-export form, §2.1–§2.4/§2.13/§2.14)
  are reached through `NODE_SPECS`/`ns_prefix_for`. They are **always-on
  ambient prelude** — either a `PreludeTs` entry (`registry_build.rs`'s
  `PreludeTs` list already includes `structuredClone`, `DOMException`,
  `performance`, `timers`, `web-api`, `events`, `streams` unconditionally in
  every compiled program) or a `GlobalClassSpec` registered the same way
  `Number`/`String` statics are. This matches real Node's own model — these
  are base-realm globals, not `node:module` exports, even though several
  Node core modules *also* re-export the identical object.
- **The few `node:*` re-export forms must preserve object identity**, exactly
  like `events.md` §5.6's finding for `EventTarget`/`Event`/`CustomEvent`: a
  **source-level** TypeScript re-export (`export { Headers, FormData, Blob,
  File, Request, Response };` inside `node:buffer`'s or a hypothetical
  `node:fetch`-shaped shim, referencing the already-in-scope ambient
  identifiers) creates **zero Rust-level crate dependency**, while
  `instanceof` checks work identically regardless of which specifier a
  caller imported from.
- **Native-extern vs `.ts`-shim split**: see §5.1's two-cluster table. The
  pure-`.ts` cluster is 100% shim/0% extern (same pattern `node:events`
  established); the native cluster (URL/TextEncoder/fetch/crypto) is a thin
  extern layer plus a `.ts` ergonomic wrapper only where one is needed beyond
  what the native class already exposes directly.

### 5.7 Shared-infra dependencies (FLAG)

Because most of this doc's surface is classified **rts-std web-global**, not
**rts-node**, the "rts-node cannot depend on rts-std" constraint mostly does
**not** bite here directly — flagged explicitly so a future implementer
doesn't go looking for a violation that isn't one. The real flags:

- **Microtask/event-loop drain primitive.** Needed by `queueMicrotask`
  directly, by `fetch()`'s `Promise` settlement, by `AbortSignal.timeout()`'s
  underlying `setTimeout`, and by `SubtleCrypto`'s mandatory `Promise`
  wrapping. Lives in `rts-std/src/event_loop.rs` + `rts-std/src/promise/mod.rs`
  today. Per `architecture.md` §7/§3.2, this must be hoisted into a shared low
  crate (`rts-async`) beneath the `rts-std` cut line — **not** because this
  doc's own (mostly rts-std-resident) classes need it there, but because the
  **rts-node**-owned siblings that also need it (`node:timers`, `node:events`'
  `once`/`on`, `node:worker_threads`) cannot reach it inside `rts-std` once
  `rts-node` is cut over to independence. This doc's classes can keep using
  it directly inside `rts-std`/`rts-shared` either way.
- **Shared tokio runtime (`rt()`, `rts-std/src/runtime/async_rt.rs`).** Needed
  by `fetch()`'s real network I/O. Already reachable from `rts-std`
  (no violation for this doc's own surface); flagged because `rts-node`'s own
  future `node:http`/`node:net` will need the *same* runtime and cannot get it
  through `rts-std` post-cutover — same hoist target as above.
- **TLS (`rustls`) + `net` (`std::net`).** Needed by `fetch()` for `https://`
  URLs. Already inside `rts-std` (`rts-std/src/tls`, implicit `rts-std/src/net`
  equivalent) — no violation for this doc.
- **Low-level crypto primitives** (`rts-std/src/crypto/mod.rs`'s SHA-256,
  base64/hex, CSPRNG). The ambient `crypto` global reuses these directly (no
  violation, both live in `rts-std`); **accepted, not shared,** with
  `node:crypto`'s (rts-node) own duplicate hash/CSPRNG implementation per
  `architecture.md` §13 open decision 3.
- **No dependency** on `fs`, `child_process`, `dgram`, or `cluster` anywhere in
  this doc's surface.

### 5.8 Implementation phases

(a) **Add `CustomEvent` to `rts-shared/src/stdlib/events.ts`** — a small,
standalone patch (`class CustomEvent extends Event { detail: any; constructor(type, opts) { super(type, opts); this.detail = opts?.detail ?? null; } }`), already flagged independently by `events.md` §5.7/§5.8 — do this once, shared by both specs.

(b) **Add `BroadcastChannel` to `rts-shared/src/stdlib/events.ts`** (new gap
this doc surfaces): a same-thread stub today (a process-wide registry keyed
by channel `name`, delivering via `queueMicrotask` to every other open
`BroadcastChannel` of the same name in-process), upgraded to a real
cross-thread primitive alongside `MessagePort`'s own upgrade once
`worker_threads` lands real threads.

(c) **Reconcile the `Headers`/`FormData`/`Blob`/`File`/`Request`/`Response`
duplication** flagged throughout §2.0/§2.10: today both a native Rust cluster
(`rts-std/src/globals/{fetch,blob,form_data,headers}`) and a pure-`.ts`
value-holder cluster (`rts-shared/src/stdlib/webapi.ts`) implement
overlapping shapes of the same classes. Recommendation (needs owner
confirmation, see §7): keep the value-holder classes (`Headers`/`FormData`/
`Blob`/`File`/the *body-consumer methods* of `Request`/`Response`) as the
pure-`.ts` implementation, and narrow the native cluster to *only* what
genuinely needs Rust — `fetch()`'s actual network transport and the
`ReadableStream` chunk production feeding a real response body — deleting the
now-redundant native `Request`/`Response`/`Headers`/`FormData`/`Blob` classes
in favor of the `.ts` ones. This is a real drain/consolidation task, not
invented scope.

(d) **Implement `URLPattern`** as a `.ts` class over the existing `URL`
parser's component extraction, or a native extern if pattern-matching
performance demands it (defer the decision to a profiling pass).

(e) **Extend `crypto.subtle`** (`SubtleCrypto`) per `crypto.md`'s own
implementation phases — this doc only needs the ambient-global wiring
(`globalThis.crypto` singleton pointing at the same `Crypto` instance
`node:crypto`'s `.webcrypto` exposes).

(f) **`WebAssembly`** — scope a follow-up spec once an RTS-native WASM
execution strategy is chosen (§7); not started.

(g) **`navigator`/`localStorage`/`sessionStorage`/`Storage`,
`WebSocket`/`EventSource`** — low-priority P2-shaped follow-ups; implement
after the above.

## 6. Test plan

```
tests/globals/abort_controller.test.ts
  - new AbortController().signal.aborted === false
  - controller.abort() sets aborted=true, reason defaults to a DOMException("...", "AbortError")
  - controller.abort("custom") preserves the exact reason value (not wrapped)
  - a second abort() call is a no-op (reason/aborted do not change)
  - signal.addEventListener('abort', fn) + signal.onabort both fire, exactly once
  - AbortSignal.abort() returns an already-aborted signal
  - AbortSignal.timeout(10) aborts asynchronously with a TimeoutError DOMException
  - AbortSignal.any([a, b]) aborts when either input aborts, propagating its reason
  - AbortSignal.any([already-aborted, b]) aborts immediately, synchronously
  - signal.throwIfAborted() throws the reason iff aborted, else returns undefined
  - fetch(url, { signal: controller.signal }) rejects when aborted mid-request

tests/globals/url.test.ts
  - new URL("https://user:pass@host:8080/path?a=1&a=2#frag") — every component readable
  - new URL("/relative", "https://example.com/base/") resolves against base
  - new URL("not a url") throws TypeError
  - URL.canParse / URL.parse happy + invalid-input paths
  - url.searchParams is a live view: mutating searchParams updates url.search
  - URLSearchParams from string / object / array-of-pairs / another URLSearchParams
  - usp.getAll returns all values for repeated keys; usp.sort() orders by name
  - usp.delete(name, value) / has(name, value) two-arg filtered forms
  - round-trip: new URL(u.toString()).href === u.href

tests/globals/text_encoding.test.ts
  - encoder.encode("héllo") produces the correct UTF-8 byte sequence (multi-byte + ASCII mix)
  - encoder.encodeInto writes into a pre-allocated buffer, reports read/written correctly
  - decoder.decode(bytes) round-trips encode/decode for ASCII, Latin-1-range, and
    4-byte-surrogate-pair codepoints
  - decoder with fatal:true throws TypeError on malformed input; fatal:false substitutes U+FFFD
  - decoder.decode(chunk1, {stream:true}) + decoder.decode(chunk2) correctly reassembles a
    multi-byte character split across two chunks

tests/globals/fetch_stack.test.ts
  - fetch(url) against a local RTS-native HTTP test server, await res.text()/.json()
  - fetch with { method: "POST", body: JSON.stringify(...), headers: {"content-type":"application/json"} }
  - new Headers([["a","1"],["a","2"]]).get("a") === "1, 2"; getSetCookie() keeps entries separate
  - new FormData() append/get/getAll/set semantics (set drops duplicates, keeps first slot)
  - new Request(url, { method: "POST", body: "x" }).clone() — both readable independently
  - Response.json({a:1}) has content-type application/json and correct body
  - Response.redirect(url, 301) — status/headers correct
  - fetch rejects with TypeError on a malformed URL and on a network error (connection refused)
  - req.bodyUsed transitions to true after first body-consumer call; a second call rejects

tests/globals/structured_clone.test.ts
  - structuredClone(42) / ("s") / (true) / (null) — primitives pass through unchanged
  - structuredClone({a: {b: [1,2,3]}}) — deep clone, mutating the clone doesn't affect original
  - cyclic object: obj.self = obj; structuredClone(obj).self === the clone (not the original)
  - shared reference: { x: shared, y: shared } clones to a result where clone.x === clone.y
  - structuredClone(new Map([[1,"a"]])) / (new Set([1,2])) / (new Date(...)) — type preserved
  - structuredClone(arrayBuffer, { transfer: [arrayBuffer] }) detaches the source (byteLength 0)

tests/globals/queue_microtask.test.ts
  - queueMicrotask runs after current sync code, before a subsequently scheduled setTimeout(0)
  - queueMicrotask runs after process.nextTick's queue on the same turn
  - an exception thrown inside the callback surfaces as an uncaughtException, not swallowed

tests/globals/event_target.test.ts
  - addEventListener/removeEventListener/dispatchEvent basic round-trip
  - once:true listener is removed before its callback runs, fires exactly once
  - dispatchEvent returns false iff a cancelable event's preventDefault() was called
  - duplicate addEventListener(type, sameFn) registers once, not twice
  - CustomEvent carries .detail through dispatch (once implemented, phase 5.8a)

tests/globals/message_channel.test.ts
  - new MessageChannel(); port1.postMessage(x); port2.onmessage receives {data: x} (async, microtask)
  - port.close() after which further postMessage does not deliver (verify exact Node semantics)

tests/globals/dom_exception.test.ts
  - new DOMException("msg", "NotFoundError").code === 8
  - new DOMException("msg", "AbortError").code === 0 (no legacy code)
  - err.name/.message/.toString() shape
  - DOMException is NOT instanceof Error (spec quirk — assert this explicitly)

tests/globals/global_identity.test.ts
  - a top-level `let x = 1` in a module does not appear as globalThis.x
  - globalThis === global (once `global` is implemented) but global is flagged legacy
  - multi-thread (worker_threads-gated): each Worker sees its own globalThis/module registry,
    not the parent's — deferred until worker_threads lands, tracked here as a placeholder
```

## 7. Open questions / deferrals

1. **`Headers`/`FormData`/`Blob`/`File`/`Request`/`Response` dual
   implementation.** As found during this spec's research, both a native Rust
   cluster (`rts-std/src/globals/{fetch,blob,form_data,headers}`) and a
   pure-`.ts` cluster (`rts-shared/src/stdlib/webapi.ts`) currently define
   overlapping shapes of the same classes, and it is not this doc's place to
   silently pick a winner. §5.8(c) proposes consolidating onto the `.ts`
   cluster plus a narrowed native transport layer — **needs explicit owner
   confirmation** before executing (per `CLAUDE.md`'s "ask before violating/
   silently resolving an architectural ambiguity" instruction).
2. **`WebAssembly`.** No implementation strategy chosen yet. Candidate:
   `cranelift-wasm` (RTS already depends on Cranelift) as an RTS-native WASM
   front-end, rather than embedding any V8-derived engine. Deferred pending a
   dedicated design doc — out of scope to design here.
3. **`BroadcastChannel` cross-thread semantics.** Not implementable for real
   until `worker_threads` maps a `Worker` onto an actual RTS thread/region
   (blocked on the same T1/T-series threading-model prerequisites
   `worker_threads.md` §5.4 lists). The same-thread `.ts` stub (§5.8(b)) is a
   correctness placeholder, not the target implementation.
4. **`navigator`/`localStorage`/`sessionStorage`.** Low value for a
   server/CLI-shaped runtime like RTS (no natural browser "origin"); deferred
   indefinitely unless a concrete use case emerges. `navigator.locks`
   (`LockManager`) overlaps with `worker_threads.md`'s own `Lock`/
   `LockManager` — when implemented, must be the *same* class, not a
   parallel one.
5. **`WebSocket`/`EventSource`.** Deferred to a P1/P2 pass; needs its own
   client over `rts-std`'s `net`/`tls`, independent of the `undici` Node uses
   internally.
6. **`URLPattern`.** Experimental even in real Node (v25.x); implement after
   the core `URL`/`fetch` surface is solid — `.ts`-over-`URL` vs native
   extern is an open performance-vs-simplicity call, not urgent.
7. **`CloseEvent`/`ErrorEvent`'s exact "since" version.** Marked `(verify)` in
   §2.5 — Node's own docs don't clearly date these outside the `WebSocket`
   global's own changelog; confirm against the v25.x doc build before
   shipping the version metadata verbatim into user-facing docs.
8. **`Request.prototype.bytes()`/`Response.prototype.bytes()`.** Marked
   `(verify)` in §2.10 — a newer addition to the Fetch spec/Node API surface;
   confirm exact Node version before committing to the signature table.
