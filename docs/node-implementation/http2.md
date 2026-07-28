# node:http2

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:http2` |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P2 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import http2 from "node:http2"`; `import { createServer, createSecureServer, connect, constants, getDefaultSettings, getPackedSettings, getUnpackedSettings, performServerHandshake, sensitiveHeaders } from "node:http2"` |
| Globals exposed | None (all surface is module-scoped; no JS global is added) |

---

## 1. Purpose

`node:http2` provides a native HTTP/2 (RFC 9113) client and server. It exposes
two layers: a low-level **Core API** (`Http2Session`/`Http2Stream` and
subclasses) that models HTTP/2's real wire concepts — multiplexed streams over
one connection, HPACK header compression, per-stream/per-session flow control,
server push, `SETTINGS`/`PING`/`GOAWAY`/`ALTSVC`/`ORIGIN` frames, and the
extended `CONNECT` method (RFC 8441) — and a higher-level **Compatibility
API** (`Http2ServerRequest`/`Http2ServerResponse`) that mirrors `node:http`'s
request/response object model so simple servers can move between HTTP/1.1 and
HTTP/2 with minimal code changes. RTS implements this on its own HTTP/2
stack (framing + HPACK + flow control), independent of `rts-std`'s
`http_server` (actix-web) namespace, matching the independence decision
already applied to `node:http`.

---

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `class Http2Session`
Extends: `EventEmitter`. Never constructed directly by user code (obtained via
`'stream'`/`'session'` events server-side, or as the return value of
`http2.connect()` client-side, always as a `ServerHttp2Session`/
`ClientHttp2Session` subclass instance).

**Instance properties**
| Property | Type | Notes |
|---|---|---|
| `alpnProtocol` | `string \| undefined` | read-only; negotiated ALPN protocol, set only after `'connect'` |
| `closed` | `boolean` | read-only |
| `connecting` | `boolean` | read-only |
| `destroyed` | `boolean` | read-only |
| `encrypted` | `boolean \| undefined` | read-only; `true` for TLS-backed sessions |
| `localSettings` | `SettingsObject` | read-only |
| `originSet` | `string[] \| undefined` | read-only; client secure sessions only |
| `pendingSettingsAck` | `boolean` | read-only |
| `remoteSettings` | `SettingsObject` | read-only |
| `socket` | `net.Socket \| tls.TLSSocket` | read-only; a **proxy** object — direct `destroy`/`emit`/`end`/`pause`/`read`/`resume`/`write` throw `ERR_HTTP2_NO_SOCKET_MANIPULATION` |
| `state` | `Http2SessionState` | read-only |
| `type` | `number` | read-only; `constants.NGHTTP2_SESSION_SERVER` (1) or `constants.NGHTTP2_SESSION_CLIENT` (0) |

**Instance methods**
| Method | Signature | Returns |
|---|---|---|
| `close` | `close(callback?: () => void): void` | `void` |
| `destroy` | `destroy(error?: Error, code?: number): void` | `void` |
| `goaway` | `goaway(code?: number, lastStreamID?: number, opaqueData?: Buffer \| TypedArray \| DataView): void` | `void` |
| `ping` | `ping(payload?: Buffer \| TypedArray \| DataView, callback: (err: Error \| null, duration: number, payload: Buffer) => void): boolean` | `boolean` (`false` if the ping could not be queued, e.g. `maxOutstandingPings` reached) |
| `ref` | `ref(): void` | `void` |
| `setLocalWindowSize` | `setLocalWindowSize(windowSize: number): void` | `void` |
| `setTimeout` | `setTimeout(msecs: number, callback?: () => void): void` | `void` |
| `settings` | `settings(settings?: SettingsObject, callback?: (err: Error \| null, settings: SettingsObject, duration: number) => void): void` | `void` |
| `unref` | `unref(): void` | `void` |

**Events**
| Event | Callback | Notes |
|---|---|---|
| `'close'` | `() => void` | |
| `'connect'` | `(session: Http2Session, socket: net.Socket \| tls.TLSSocket) => void` | |
| `'error'` | `(error: Error) => void` | |
| `'frameError'` | `(type: number, code: number, id: number) => void` | frame failed to be serialized/sent |
| `'goaway'` | `(errorCode: number, lastStreamID: number, opaqueData: Buffer) => void` | |
| `'localSettings'` | `(settings: SettingsObject) => void` | local `SETTINGS` acknowledged |
| `'ping'` | `(payload: Buffer) => void` | peer `PING` received |
| `'remoteSettings'` | `(settings: SettingsObject) => void` | |
| `'stream'` | `(stream: Http2Stream, headers: HeadersObject, flags: number, rawHeaders: string[]) => void` | new stream opened by the peer |
| `'timeout'` | `() => void` | idle timeout (see `setTimeout`) |

---

#### `class ServerHttp2Session`
Extends: `Http2Session`.

**Instance methods**
| Method | Signature | Returns |
|---|---|---|
| `altsvc` | `altsvc(alt: string, originOrStream: number \| string \| URL \| { host?: string, port?: number, protocol?: string }): void` | `void` |
| `origin` | `origin(...origins: (string \| URL \| { protocol?: string, host?: string, port?: number })[]): void` | `void` |

**Events:** inherits all `Http2Session` events; no new ones.

---

#### `class ClientHttp2Session`
Extends: `Http2Session`.

**Instance methods**
| Method | Signature | Returns |
|---|---|---|
| `request` | `request(headers?: OutgoingHttpHeaders, options?: ClientSessionRequestOptions): ClientHttp2Stream` | `ClientHttp2Stream` |

**Events**
| Event | Callback | Notes |
|---|---|---|
| `'altsvc'` | `(alt: string, origin: string, streamId: number) => void` | |
| `'origin'` | `(origins: string[]) => void` | requires `enableConnectProtocol`-independent `origin` tracking; TLS sessions only |
(plus inherited `Http2Session` events.)

---

#### `class Http2Stream`
Extends: `stream.Duplex`. Never constructed directly by user code.

**Instance properties**
| Property | Type | Notes |
|---|---|---|
| `aborted` | `boolean` | read-only |
| `bufferSize` | `number` | read-only; bytes currently buffered for send |
| `closed` | `boolean` | read-only |
| `destroyed` | `boolean` | read-only |
| `endAfterHeaders` | `boolean` | read-only; `true` when the `END_STREAM` flag was set on the initiating `HEADERS` frame |
| `id` | `number \| undefined` | read-only |
| `pending` | `boolean` | read-only; `true` until an `id` has been assigned |
| `rstCode` | `number` | read-only; `RST_STREAM` code, `constants.NGHTTP2_NO_ERROR` if not reset |
| `sentHeaders` | `HeadersObject` | read-only |
| `sentInfoHeaders` | `HeadersObject[] \| undefined` | read-only; 1xx informational headers sent |
| `sentTrailers` | `HeadersObject \| undefined` | read-only |
| `session` | `Http2Session` | read-only |
| `state` | `Http2StreamState` | read-only |

**Instance methods**
| Method | Signature | Returns |
|---|---|---|
| `close` | `close(code?: number, callback?: () => void): void` | `void`; `code` default `constants.NGHTTP2_NO_ERROR` (`0x00`) |
| `priority` | `priority(options: StreamPriorityOptions): void` | `void`; **deprecated as of v24.2.0/v22.17.0 — no-op**, emits a runtime warning (RFC 9113 removed frame-based priority) |
| `sendTrailers` | `sendTrailers(headers: HeadersObject): void` | `void` |
| `setTimeout` | `setTimeout(msecs: number, callback?: () => void): void` | `void` |

**Events**
| Event | Callback | Notes |
|---|---|---|
| `'aborted'` | `() => void` | |
| `'close'` | `() => void` | |
| `'error'` | `(error: Error) => void` | |
| `'frameError'` | `(type: number, code: number, id: number) => void` | |
| `'ready'` | `() => void` | stream `id` assigned, stream usable |
| `'timeout'` | `() => void` | |
| `'trailers'` | `(headers: HeadersObject, flags: number) => void` | |
| `'wantTrailers'` | `() => void` | fires when `waitForTrailers` was requested and the stream is ready for `sendTrailers` |
(plus inherited `stream.Duplex` events: `'data'`, `'end'`, `'drain'`, `'finish'`, `'pause'`, `'readable'`, `'resume'`.)

---

#### `class ClientHttp2Stream`
Extends: `Http2Stream`. Returned by `ClientHttp2Session.request()`.

**Events**
| Event | Callback | Notes |
|---|---|---|
| `'continue'` | `() => void` | server sent a `100` informational response |
| `'headers'` | `(headers: HeadersObject, flags: number, rawHeaders: string[]) => void` | additional (non-100) 1xx informational headers |
| `'push'` | `(headers: HeadersObject, flags: number) => void` | server-initiated push promise headers |
| `'response'` | `(headers: HeadersObject, flags: number, rawHeaders: string[]) => void` | final response headers |
(plus inherited `Http2Stream` events.)

---

#### `class ServerHttp2Stream`
Extends: `Http2Stream`. Delivered via the server's `'stream'` event.

**Instance properties**
| Property | Type | Notes |
|---|---|---|
| `headersSent` | `boolean` | read-only |
| `pushAllowed` | `boolean` | read-only; reflects the peer's `SETTINGS_ENABLE_PUSH` |

**Instance methods**
| Method | Signature | Returns |
|---|---|---|
| `additionalHeaders` | `additionalHeaders(headers: HeadersObject): void` | `void`; sends a 1xx informational `HEADERS` frame |
| `pushStream` | `pushStream(headers: HeadersObject, options?: { exclusive?: boolean, parent?: number }, callback: (err: Error \| null, pushStream: ServerHttp2Stream, headers: HeadersObject) => void): void` | `void` |
| `respond` | `respond(headers?: HeadersObject, options?: { endStream?: boolean, waitForTrailers?: boolean }): void` | `void` |
| `respondWithFD` | `respondWithFD(fd: number \| fs.promises.FileHandle, headers?: HeadersObject, options?: { statCheck?: (stat: fs.Stats, headers: HeadersObject, statOptions: object) => void, waitForTrailers?: boolean, offset?: number, length?: number }): void` | `void` |
| `respondWithFile` | `respondWithFile(path: string \| Buffer \| URL, headers?: HeadersObject, options?: { statCheck?: (stat: fs.Stats, headers: HeadersObject, statOptions: object) => void, onError?: (err: NodeJS.ErrnoException) => void, waitForTrailers?: boolean, offset?: number, length?: number }): void` | `void` |

**Events:** inherits all `Http2Stream` events; no new ones.

---

#### `class Http2Server`
Extends: `net.Server`. Constructed via `http2.createServer(...)`, never `new Http2Server(...)` directly.

**Instance properties**
| Property | Type | Notes |
|---|---|---|
| `timeout` | `number` | milliseconds; `0` = no timeout |

**Instance methods**
| Method | Signature | Returns |
|---|---|---|
| `close` | `close(callback?: (err?: Error) => void): void` | `void` |
| `setTimeout` | `setTimeout(msecs?: number, callback?: () => void): this` | `this` |
| `updateSettings` | `updateSettings(settings?: SettingsObject): void` | `void`; changes the settings sent on future connections |
| `[Symbol.asyncDispose]` | `(): Promise<void>` | disposes via `close()` |

**Events**
| Event | Callback | Notes |
|---|---|---|
| `'checkContinue'` | `(request: Http2ServerRequest, response: Http2ServerResponse) => void` | Compatibility API only |
| `'connection'` | `(socket: net.Socket) => void` | |
| `'request'` | `(request: Http2ServerRequest, response: Http2ServerResponse) => void` | Compatibility API |
| `'session'` | `(session: ServerHttp2Session) => void` | new session established |
| `'sessionError'` | `(error: Error) => void` | |
| `'stream'` | `(stream: ServerHttp2Stream, headers: HeadersObject, flags: number, rawHeaders: string[]) => void` | Core API |
| `'timeout'` | `() => void` | |

---

#### `class Http2SecureServer`
Extends: `tls.Server`. Constructed via `http2.createSecureServer(...)`.

**Instance properties, methods, events:** identical to `Http2Server`, plus:

**Events (additional)**
| Event | Callback | Notes |
|---|---|---|
| `'unknownProtocol'` | `(socket: tls.TLSSocket) => void` | ALPN negotiation did not select `h2`/`http/1.1` and `allowHTTP1` did not apply |

---

#### `class Http2ServerRequest` (Compatibility API)
Extends: `stream.Readable`. Constructed internally, passed as arg 1 of `'request'`.

**Instance properties**
| Property | Type | Notes |
|---|---|---|
| `aborted` | `boolean` | read-only |
| `authority` | `string` | read-only; `:authority` pseudo-header |
| `complete` | `boolean` | read-only |
| `connection` | `net.Socket \| tls.TLSSocket` | **deprecated**, alias of `socket` |
| `headers` | `Record<string, string \| string[]>` | read-only |
| `httpVersion` | `string` | read-only; always `"2.0"` |
| `method` | `string` | read-only; `:method` pseudo-header |
| `rawHeaders` | `string[]` | read-only |
| `rawTrailers` | `string[]` | read-only |
| `scheme` | `string` | read-only; `:scheme` pseudo-header |
| `socket` | `net.Socket \| tls.TLSSocket` | read-only |
| `stream` | `Http2Stream` | read-only; the underlying Core API stream |
| `trailers` | `Record<string,string>` | read-only |
| `url` | `string` | read-only; `:path` pseudo-header |

**Instance methods**
| Method | Signature | Returns |
|---|---|---|
| `destroy` | `destroy(error?: Error): void` | `void` |
| `setTimeout` | `setTimeout(msecs: number, callback: () => void): void` | `void` |

**Events**
| Event | Callback |
|---|---|
| `'aborted'` | `() => void` |
| `'close'` | `() => void` |
(plus inherited `stream.Readable` events.)

---

#### `class Http2ServerResponse` (Compatibility API)
Extends: `stream.Writable`. Constructed internally, passed as arg 2 of `'request'`.

**Instance properties**
| Property | Type | Notes |
|---|---|---|
| `connection` | `net.Socket \| tls.TLSSocket` | **deprecated**, alias of `socket` |
| `finished` | `boolean` | **deprecated** — use `writableEnded` |
| `headersSent` | `boolean` | read-only |
| `req` | `Http2ServerRequest` | read-only |
| `sendDate` | `boolean` | default `true` |
| `socket` | `net.Socket \| tls.TLSSocket` | read-only |
| `statusCode` | `number` | default `200` |
| `statusMessage` | `string` | **always ignored** — HTTP/2 has no reason-phrase on the wire; setting it is a silent no-op |
| `stream` | `Http2Stream` | read-only |
| `writableEnded` | `boolean` | read-only |

**Instance methods**
| Method | Signature | Returns |
|---|---|---|
| `addTrailers` | `addTrailers(headers: Record<string,string>): void` | `void` |
| `appendHeader` | `appendHeader(name: string, value: string \| string[]): void` | `void` |
| `createPushResponse` | `createPushResponse(headers: OutgoingHttpHeaders, callback: (err: Error \| null, res: Http2ServerResponse) => void): void` | `void` |
| `end` | `end(data?: string \| Buffer, encoding?: BufferEncoding, callback?: () => void): void` | `void` |
| `getHeader` | `getHeader(name: string): string \| string[] \| undefined` | value |
| `getHeaderNames` | `getHeaderNames(): string[]` | `string[]` |
| `getHeaders` | `getHeaders(): Record<string, string \| string[]>` | `Object` |
| `hasHeader` | `hasHeader(name: string): boolean` | `boolean` |
| `removeHeader` | `removeHeader(name: string): void` | `void` |
| `setHeader` | `setHeader(name: string, value: string \| string[]): void` | `void` |
| `setTimeout` | `setTimeout(msecs: number, callback?: () => void): void` | `void` |
| `write` | `write(chunk: string \| Buffer, encoding?: BufferEncoding, callback?: () => void): boolean` | `boolean` |
| `writeContinue` | `writeContinue(): void` | `void`; sends a `100` informational response |
| `writeEarlyHints` | `writeEarlyHints(hints: Record<string, string \| string[]>): void` | `void`; sends a `103` informational response |
| `writeHead` | `writeHead(statusCode: number, statusMessage?: string, headers?: OutgoingHttpHeaders): this` <br> `writeHead(statusCode: number, headers?: OutgoingHttpHeaders): this` | `this`; `statusMessage` accepted for signature parity with `http.ServerResponse` but discarded on the wire |

**Events**
| Event | Callback |
|---|---|
| `'close'` | `() => void` |
| `'finish'` | `() => void` |

---

### 2.2 Top-level functions

#### `createServer`
```ts
http2.createServer(options？: ServerOptions, onRequestHandler？: (request: Http2ServerRequest, response: Http2ServerResponse) => void): Http2Server
```
| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `ServerOptions` | yes | `{}` |
| `onRequestHandler` | `(request, response) => void` | yes | none — attach via `server.on('request', ...)` or use only the Core API's `'stream'` event |

Returns: `Http2Server` (unencrypted, `h2c`; not yet listening).
Throws: `TypeError` (`ERR_INVALID_ARG_TYPE`) for malformed option types.
Variant: **sync constructor** (I/O is callback/event-driven).

#### `createSecureServer`
```ts
http2.createSecureServer(options: SecureServerOptions, onRequestHandler？: (request: Http2ServerRequest, response: Http2ServerResponse) => void): Http2SecureServer
```
| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `SecureServerOptions` | no (TLS `key`/`cert` normally required) | — |
| `onRequestHandler` | same as above | yes | none |

Returns: `Http2SecureServer` (TLS-backed, ALPN-negotiated `h2` or, with
`allowHTTP1: true`, falls back to HTTP/1.1).
Throws: TLS option errors surface synchronously (`ERR_INVALID_ARG_TYPE`,
`ERR_TLS_*`); handshake failures are async via `'error'`/`'unknownProtocol'`.
Variant: **sync constructor**.

#### `connect`
```ts
http2.connect(authority: string | URL | { host?: string, port?: number }, options？: ClientSessionOptions, listener？: (session: ClientHttp2Session, socket: net.Socket | tls.TLSSocket) => void): ClientHttp2Session
```
| Param | Type | Optional | Default |
|---|---|---|---|
| `authority` | `string \| URL \| Object` | no | — |
| `options` | `ClientSessionOptions` | yes | `{}` |
| `listener` | `(session, socket) => void` | yes | none — attach via `session.on('connect', ...)` |

Returns: `ClientHttp2Session` (connecting asynchronously; usable immediately,
requests queue until `'connect'`).
Throws: `TypeError` for a malformed `authority`; connection failures surface
async via `'error'`; requesting on an already-`goaway`'d session throws
`ERR_HTTP2_GOAWAY_SESSION`.
Variant: **callback/event** (the session itself is returned synchronously; readiness is event-driven).

#### `getDefaultSettings`
```ts
http2.getDefaultSettings(): SettingsObject
```
Returns the built-in default `SettingsObject` (see §3). No params, no throw.
Variant: **sync**.

#### `getPackedSettings`
```ts
http2.getPackedSettings(settings？: SettingsObject): Buffer
```
| Param | Type | Optional | Default |
|---|---|---|---|
| `settings` | `SettingsObject` | yes | current default settings |

Returns: `Buffer`, 6 bytes per included setting (2-byte identifier + 4-byte
value), suitable for an `HTTP2-Settings` upgrade header or manual frame
construction.
Throws: `ERR_HTTP2_INVALID_SETTING_VALUE` for out-of-range values.
Variant: **sync**.

#### `getUnpackedSettings`
```ts
http2.getUnpackedSettings(buf: Buffer | Uint8Array): SettingsObject
```
| Param | Type | Optional | Default |
|---|---|---|---|
| `buf` | `Buffer \| Uint8Array` | no | — |

Returns: `SettingsObject`.
Throws: `RangeError` (`ERR_HTTP2_INVALID_PACKED_SETTINGS_LENGTH`) if `buf.length` is not a multiple of 6.
Variant: **sync**.

#### `performServerHandshake`
```ts
http2.performServerHandshake(socket: net.Socket | tls.TLSSocket, options？: ServerOptions): Http2Session
```
| Param | Type | Optional | Default |
|---|---|---|---|
| `socket` | `net.Socket \| tls.TLSSocket` | no | — |
| `options` | `ServerOptions` | yes | `{}` |

Returns: `Http2Session` bound to an already-accepted/upgraded socket (used to
implement HTTP/1.1→HTTP/2 `Upgrade`/prior-knowledge handoff without going
through `createServer`'s own accept loop).
Throws: `ERR_HTTP2_SOCKET_BOUND` if the socket already has an active session.
Variant: **sync constructor** (handshake itself proceeds async over the socket).

### 2.3 Properties & constants

| Name | Type | Notes |
|---|---|---|
| `http2.constants` | `Object` | see below |
| `http2.sensitiveHeaders` | `symbol` | computed key on a headers object; its value is `string[]` of header names to flag `NGHTTP2_NV_FLAG_NO_INDEXING` (never HPACK-indexed, so never cached/leaked across requests) |

**`http2.constants` contents:**

*Session type*
| Constant | Value |
|---|---|
| `NGHTTP2_SESSION_SERVER` | `0` |
| `NGHTTP2_SESSION_CLIENT` | `1` |

(Note: some Node docs list `SERVER`/`CLIENT` in the opposite 0/1 order across
versions — verify exact values against Node 25's `lib/internal/http2/constants.js` at implementation time.)

*RST_STREAM / GOAWAY error codes*
| Constant | Value |
|---|---|
| `NGHTTP2_NO_ERROR` | `0x00` |
| `NGHTTP2_PROTOCOL_ERROR` | `0x01` |
| `NGHTTP2_INTERNAL_ERROR` | `0x02` |
| `NGHTTP2_FLOW_CONTROL_ERROR` | `0x03` |
| `NGHTTP2_SETTINGS_TIMEOUT` | `0x04` |
| `NGHTTP2_STREAM_CLOSED` | `0x05` |
| `NGHTTP2_FRAME_SIZE_ERROR` | `0x06` |
| `NGHTTP2_REFUSED_STREAM` | `0x07` |
| `NGHTTP2_CANCEL` | `0x08` |
| `NGHTTP2_COMPRESSION_ERROR` | `0x09` |
| `NGHTTP2_CONNECT_ERROR` | `0x0a` |
| `NGHTTP2_ENHANCE_YOUR_CALM` | `0x0b` |
| `NGHTTP2_INADEQUATE_SECURITY` | `0x0c` |
| `NGHTTP2_HTTP_1_1_REQUIRED` | `0x0d` |

*Padding strategy (`ServerOptions.paddingStrategy` / `ClientSessionOptions.paddingStrategy`)* — (verify exact set/values against Node 25 source)
| Constant | Meaning |
|---|---|
| `PADDING_STRATEGY_NONE` | no automatic padding (default) |
| `PADDING_STRATEGY_ALIGNED` | pad frames to align on 8-byte boundaries |
| `PADDING_STRATEGY_MAX` | pad each frame to `maxFrameSize` |
| `PADDING_STRATEGY_CALLBACK` | delegate to `options.selectPadding` |

*HTTP/2 pseudo-header + common header-name constants (`HTTP2_HEADER_*`)* —
the fetched Node 25 docs only enumerate the pseudo-headers exhaustively; the
well-known-header subset below is reconstructed from Node's stable constant
table and **must be verified against `lib/internal/http2/constants.js` in the
Node 25 source before being hardcoded** (mark: **verify**):

| Constant | Wire value |
|---|---|
| `HTTP2_HEADER_STATUS` | `:status` |
| `HTTP2_HEADER_METHOD` | `:method` |
| `HTTP2_HEADER_AUTHORITY` | `:authority` |
| `HTTP2_HEADER_SCHEME` | `:scheme` |
| `HTTP2_HEADER_PATH` | `:path` |
| `HTTP2_HEADER_PROTOCOL` | `:protocol` (extended CONNECT, RFC 8441) |
| `HTTP2_HEADER_ACCEPT_ENCODING` | `accept-encoding` |
| `HTTP2_HEADER_ACCEPT_LANGUAGE` | `accept-language` |
| `HTTP2_HEADER_ACCEPT_RANGES` | `accept-ranges` |
| `HTTP2_HEADER_ACCEPT` | `accept` |
| `HTTP2_HEADER_ACCESS_CONTROL_ALLOW_CREDENTIALS` | `access-control-allow-credentials` |
| `HTTP2_HEADER_ACCESS_CONTROL_ALLOW_HEADERS` | `access-control-allow-headers` |
| `HTTP2_HEADER_ACCESS_CONTROL_ALLOW_METHODS` | `access-control-allow-methods` |
| `HTTP2_HEADER_ACCESS_CONTROL_ALLOW_ORIGIN` | `access-control-allow-origin` |
| `HTTP2_HEADER_ACCESS_CONTROL_EXPOSE_HEADERS` | `access-control-expose-headers` |
| `HTTP2_HEADER_ACCESS_CONTROL_MAX_AGE` | `access-control-max-age` |
| `HTTP2_HEADER_ACCESS_CONTROL_REQUEST_HEADERS` | `access-control-request-headers` |
| `HTTP2_HEADER_ACCESS_CONTROL_REQUEST_METHOD` | `access-control-request-method` |
| `HTTP2_HEADER_AGE` | `age` |
| `HTTP2_HEADER_AUTHORIZATION` | `authorization` |
| `HTTP2_HEADER_CACHE_CONTROL` | `cache-control` |
| `HTTP2_HEADER_CONNECTION` | `connection` (forbidden in HTTP/2 requests/responses) |
| `HTTP2_HEADER_CONTENT_DISPOSITION` | `content-disposition` |
| `HTTP2_HEADER_CONTENT_ENCODING` | `content-encoding` |
| `HTTP2_HEADER_CONTENT_LENGTH` | `content-length` |
| `HTTP2_HEADER_CONTENT_TYPE` | `content-type` |
| `HTTP2_HEADER_COOKIE` | `cookie` |
| `HTTP2_HEADER_DATE` | `date` |
| `HTTP2_HEADER_ETAG` | `etag` |
| `HTTP2_HEADER_FORWARDED` | `forwarded` |
| `HTTP2_HEADER_HOST` | `host` |
| `HTTP2_HEADER_IF_MODIFIED_SINCE` | `if-modified-since` |
| `HTTP2_HEADER_IF_NONE_MATCH` | `if-none-match` |
| `HTTP2_HEADER_IF_RANGE` | `if-range` |
| `HTTP2_HEADER_LAST_MODIFIED` | `last-modified` |
| `HTTP2_HEADER_LINK` | `link` |
| `HTTP2_HEADER_LOCATION` | `location` |
| `HTTP2_HEADER_RANGE` | `range` |
| `HTTP2_HEADER_REFERER` | `referer` |
| `HTTP2_HEADER_SERVER` | `server` |
| `HTTP2_HEADER_SET_COOKIE` | `set-cookie` |
| `HTTP2_HEADER_STRICT_TRANSPORT_SECURITY` | `strict-transport-security` |
| `HTTP2_HEADER_TRANSFER_ENCODING` | `transfer-encoding` (forbidden in HTTP/2) |
| `HTTP2_HEADER_TE` | `te` (only `trailers` value permitted in HTTP/2) |
| `HTTP2_HEADER_UPGRADE` | `upgrade` (forbidden in HTTP/2) |
| `HTTP2_HEADER_USER_AGENT` | `user-agent` |
| `HTTP2_HEADER_VARY` | `vary` |
| `HTTP2_HEADER_X_CONTENT_TYPE_OPTIONS` | `x-content-type-options` |
| `HTTP2_HEADER_X_FRAME_OPTIONS` | `x-frame-options` |
| `HTTP2_HEADER_KEEP_ALIVE` | `keep-alive` (forbidden in HTTP/2) |
| `HTTP2_HEADER_PROXY_AUTHENTICATE` | `proxy-authenticate` |
| `HTTP2_HEADER_PROXY_AUTHORIZATION` | `proxy-authorization` |
| `HTTP2_HEADER_X_XSS_PROTECTION` | `x-xss-protection` |
| `HTTP2_HEADER_ALT_SVC` | `alt-svc` |

*HTTP method constants (`HTTP2_METHOD_*`)* (confirmed exhaustive from the fetched source):
`ACL, BASELINE-CONTROL, BIND, CHECKIN, CHECKOUT, CONNECT, COPY, DELETE, GET,
HEAD, LABEL, LOCK, MERGE, MKACTIVITY, MKCALENDAR, MKCOL, MKREDIRECTREF,
MKWORKSPACE, MOVE, OPTIONS, ORDERPATCH, PATCH, POST, PRI, PROPFIND,
PROPPATCH, PUT, REPORT, SEARCH, TRACE, UNCHECKOUT, UNLINK, UNLOCK, UPDATE,
VERSION-CONTROL` — each exposed as `HTTP2_METHOD_<NAME_WITH_UNDERSCORES>`.

*HTTP status constants (`HTTP_STATUS_*`)* (confirmed exhaustive from the
fetched source): `CONTINUE 100, SWITCHING_PROTOCOLS 101, OK 200, CREATED 201,
ACCEPTED 202, NON_AUTHORITATIVE_INFORMATION 203, NO_CONTENT 204,
RESET_CONTENT 205, PARTIAL_CONTENT 206, MULTIPLE_CHOICES 300,
MOVED_PERMANENTLY 301, FOUND 302, SEE_OTHER 303, NOT_MODIFIED 304,
USE_PROXY 305, TEMPORARY_REDIRECT 307, PERMANENT_REDIRECT 308,
BAD_REQUEST 400, UNAUTHORIZED 401, PAYMENT_REQUIRED 402, FORBIDDEN 403,
NOT_FOUND 404, METHOD_NOT_ALLOWED 405, NOT_ACCEPTABLE 406,
PROXY_AUTHENTICATION_REQUIRED 407, REQUEST_TIMEOUT 408, CONFLICT 409,
GONE 410, LENGTH_REQUIRED 411, PRECONDITION_FAILED 412,
PAYLOAD_TOO_LARGE 413, URI_TOO_LONG 414, UNSUPPORTED_MEDIA_TYPE 415,
RANGE_NOT_SATISFIABLE 416, EXPECTATION_FAILED 417, TEAPOT 418,
MISDIRECTED_REQUEST 421, UNPROCESSABLE_ENTITY 422, LOCKED 423,
FAILED_DEPENDENCY 424, TOO_EARLY 425, UPGRADE_REQUIRED 426,
PRECONDITION_REQUIRED 428, TOO_MANY_REQUESTS 429,
REQUEST_HEADER_FIELDS_TOO_LARGE 431, UNAVAILABLE_FOR_LEGAL_REASONS 451,
INTERNAL_SERVER_ERROR 500, NOT_IMPLEMENTED 501, BAD_GATEWAY 502,
SERVICE_UNAVAILABLE 503, GATEWAY_TIMEOUT 504,
HTTP_VERSION_NOT_SUPPORTED 505, VARIANT_ALSO_NEGOTIATES 506,
INSUFFICIENT_STORAGE 507, LOOP_DETECTED 508, NOT_EXTENDED 510,
NETWORK_AUTHENTICATION_REQUIRED 511`.

### 2.4 Events

Per-instance, not module-scoped (see class tables above). Cross-reference:

| Class | Events |
|---|---|
| `Http2Session` | `close`, `connect`, `error`, `frameError`, `goaway`, `localSettings`, `ping`, `remoteSettings`, `stream`, `timeout` |
| `ClientHttp2Session` | `altsvc`, `origin` (+ inherited) |
| `Http2Stream` | `aborted`, `close`, `error`, `frameError`, `ready`, `timeout`, `trailers`, `wantTrailers` |
| `ClientHttp2Stream` | `continue`, `headers`, `push`, `response` (+ inherited) |
| `Http2Server` / `Http2SecureServer` | `checkContinue`, `connection`, `request`, `session`, `sessionError`, `stream`, `timeout` (+ `unknownProtocol` on `Http2SecureServer`) |
| `Http2ServerRequest` | `aborted`, `close` (+ inherited Readable) |
| `Http2ServerResponse` | `close`, `finish` |

---

## 3. Types & option objects

```ts
type BufferEncoding =
  | "ascii" | "utf8" | "utf-8" | "utf16le" | "utf-16le" | "ucs2" | "ucs-2"
  | "base64" | "base64url" | "latin1" | "binary" | "hex";

interface SettingsObject {
  headerTableSize?: number;          // 0-4294967295, default 4096
  enablePush?: boolean;               // default true
  initialWindowSize?: number;         // 0-2147483647, default 65535
  maxFrameSize?: number;              // 16384-16777215, default 16384
  maxConcurrentStreams?: number;      // 0-4294967295, default Infinity
  maxHeaderListSize?: number;         // 0-4294967295, default 65535
  enableConnectProtocol?: boolean;    // default false (RFC 8441)
}

interface Http2SessionOptions {
  maxDeflateDynamicTableSize?: number;  // default 4 * 1024
  maxSessionMemory?: number;            // default 10 (in units of 1_000_000 bytes)
  maxHeaderListPairs?: number;          // default 128
  maxOutstandingPings?: number;         // default 10
  maxSendHeaderBlockLength?: number;    // no default cap unless set
  paddingStrategy?: number;             // constants.PADDING_STRATEGY_*, default NONE
  peerMaxConcurrentStreams?: number;    // default 100
  selectPadding?: (frameLen: number, maxFrameLen: number) => number; // used iff paddingStrategy === PADDING_STRATEGY_CALLBACK
  settings?: SettingsObject;
  unknownProtocolTimeout?: number;      // default 10000 (secure server ALPN fallback wait)
  streamResetBurst?: number;            // (verify) rapid-reset (CVE-2023-44487) mitigation: allowed burst of stream resets
  streamResetRate?: number;             // (verify) rapid-reset mitigation: sustained resets/sec threshold before GOAWAY
}

interface ServerOptions extends Http2SessionOptions {
  Http1IncomingMessage？: typeof Http2ServerRequest;   // custom subclass, allowHTTP1 fallback
  Http1ServerResponse？: typeof Http2ServerResponse;   // custom subclass, allowHTTP1 fallback
  Http2ServerRequest？: typeof Http2ServerRequest;     // custom subclass factory
  Http2ServerResponse？: typeof Http2ServerResponse;   // custom subclass factory
  origins?: string[];                                  // pre-advertised via an ORIGIN frame after connect
}

interface SecureServerOptions extends ServerOptions {
  allowHTTP1?: boolean;               // default false; ALPN fallback to HTTP/1.1
  // + the full tls.createServer() surface: key, cert, ca, ciphers, ALPNCallback,
  // handshakeTimeout, sessionTimeout, ticketKeys, honorCipherOrder,
  // requestCert, rejectUnauthorized, SNICallback, minVersion/maxVersion, etc.
}

interface ClientSessionOptions extends Http2SessionOptions {
  createConnection?: (authority: URL, options: ClientSessionOptions) => stream.Duplex;
  protocol?: "http:" | "https:";      // default "https:"
  maxReservedRemoteStreams?: number;  // cap on concurrently-reserved (pushed) streams
  // + the full net.connect()/tls.connect() surface: host, port, path,
  // socketPath, rejectUnauthorized, ca, cert, key, servername, timeout, etc.
}

interface ClientSessionRequestOptions {
  endStream?: boolean;                // default false unless the method has no body (GET/HEAD/...)
  exclusive?: boolean;                // deprecated priority-tree hint, no-op since RFC 9113 alignment
  parent?: number;                    // deprecated priority-tree hint, no-op
  weight?: number;                    // deprecated (RFC 9113), triggers a runtime warning if set
  waitForTrailers?: boolean;          // default false
  signal?: AbortSignal;
}

interface StreamPriorityOptions {     // accepted for signature compat; body is a no-op since v24.2.0/v22.17.0
  parent？: number;
  weight？: number;
  exclusive？: boolean;
  silent？: boolean;
}

interface Http2SessionState {
  effectiveLocalWindowSize: number;
  effectiveRecvDataLength: number;
  nextStreamID: number;
  localWindowSize: number;
  lastProcStreamID: number;
  remoteWindowSize: number;
  outboundQueueSize: number;
  deflateDynamicTableSize: number;
  inflateDynamicTableSize: number;
}

interface Http2StreamState {
  localWindowSize: number;
  state: number;               // internal nghttp2 stream-state enum
  localClose: number;          // 0 | 1
  remoteClose: number;         // 0 | 1
  sumDependencyWeight: number; // always 0 post RFC-9113 alignment
  weight: number;              // always 16 post RFC-9113 alignment
}

type HeadersObject = {
  [pseudoOrName: string]: string | string[] | number | undefined;
  [http2.sensitiveHeaders]?: string[];
};

type OutgoingHttpHeaders = HeadersObject;

interface ServerStreamRespondOptions {
  endStream?: boolean;
  waitForTrailers?: boolean;
}

interface ServerStreamFileOptions {
  statCheck?: (stat: fs.Stats, headers: HeadersObject, statOptions: { offset: number, length: number }) => void | boolean;
  onError?: (err: NodeJS.ErrnoException) => void;   // respondWithFile only
  waitForTrailers?: boolean;
  offset?: number;
  length?: number;
}
```

---

## 4. Node semantics & edge cases

- **Pseudo-headers.** `:method`/`:scheme`/`:authority`/`:path` (and
  `:protocol` for extended CONNECT) must appear in a headers object passed to
  `request()`/`respond()`; sending one outside its allowed position throws
  `ERR_HTTP2_INVALID_PSEUDOHEADER`/`ERR_HTTP2_PSEUDOHEADER_NOT_ALLOWED`.
  Regular headers are matched **case-insensitively but always transmitted
  lowercase** — HTTP/2 forbids mixed-case header names on the wire; Node
  lowercases automatically, RTS must too.
- **Forbidden HTTP/1-only headers.** `connection`, `keep-alive`,
  `transfer-encoding`, `upgrade` are not permitted in HTTP/2 messages (their
  semantics are subsumed by the framing layer); `te` is allowed only with the
  value `trailers`.
- **`:authority` vs `host`.** Since Node v15, requests carrying a `host`
  header (with or without `:authority`) are accepted; Node maps between them
  for Compatibility API consumers (`request.authority` always reflects the
  effective value). RTS should normalize the same way rather than requiring
  callers to pick one.
- **HPACK dynamic table.** `maxDeflateDynamicTableSize` bounds the **local**
  (outbound/compression) table only; the **inbound** table size is dictated by
  the peer's advertised `headerTableSize` setting — RTS's HPACK encoder/decoder
  must track these independently per direction, per session.
- **`maxHeaderListPairs`/`maxHeaderListSize`.** Exceeding either during
  decompression is a protocol-level defense against header-based memory
  exhaustion; Node aborts the stream (not the whole session) with a
  `RST_STREAM`/`ERR_HTTP2_...` surfaced as a stream `'error'`.
- **Padding strategy.** `PADDING_STRATEGY_NONE` (default, no padding),
  `PADDING_STRATEGY_ALIGNED` (align to 8 bytes), `PADDING_STRATEGY_MAX` (pad
  to `maxFrameSize`), `PADDING_STRATEGY_CALLBACK` (`selectPadding` decides per
  frame) — all purely a bandwidth/traffic-analysis-resistance tradeoff, never
  observable at the framed-payload level.
- **Priority signaling is dead code, by spec.** RFC 9113 (obsoleting RFC 7540)
  removed frame-based stream dependency/weight. As of Node v24.2.0/v22.17.0:
  `request()`'s `weight`/`exclusive`/`parent` options and
  `http2stream.priority()` are **no-ops** (with a deprecation warning);
  `state.weight`/`state.sumDependencyWeight` are pinned to `16`/`0`. RTS's
  implementation should treat these as accepted-but-inert from day one — do
  not build a real priority tree.
- **Extended CONNECT (RFC 8441).** Requires `enableConnectProtocol: true` on
  **both** the server's `SettingsObject` and the session that wants to use
  `:protocol` (e.g. `'websocket'`) — a `CONNECT` request with `:protocol` sent
  before the server has acknowledged `enableConnectProtocol` is rejected. This
  is the mechanism WebSocket-over-HTTP/2 (RFC 9220) is built on.
- **ALPN / h2c / `allowHTTP1`.** `createSecureServer` negotiates `h2` (or,
  with `allowHTTP1: true`, may fall back to `http/1.1` and hand the connection
  to an HTTP/1.1-shaped path using the `Http1IncomingMessage`/
  `Http1ServerResponse` classes). Plain `createServer` only ever speaks
  cleartext HTTP/2 (`h2c`) — no browser negotiates `h2c`, so it is used for
  server-to-server or prior-knowledge (`http2.connect` with `protocol:
  'http:'`) traffic only.
- **`sensitiveHeaders`.** The `http2.sensitiveHeaders` symbol, when used as a
  key on an outgoing headers object with a `string[]` value, marks those
  header names as HPACK "never indexed" (`NGHTTP2_NV_FLAG_NO_INDEXING`) —
  they are sent without HPACK compression to avoid a shared-dictionary
  side-channel leaking `authorization`/`cookie` values across streams (a real
  historical HTTP/2 timing-attack concern).
- **`session.socket` is a controlled proxy.** Calling `destroy`/`emit`/`end`/
  `pause`/`read`/`resume`/`write` on it directly throws
  `ERR_HTTP2_NO_SOCKET_MANIPULATION` — all lifecycle control must go through
  the `Http2Session`/`Http2Stream` API so the multiplexed framing state stays
  consistent.
- **GOAWAY / graceful shutdown.** `session.close()` waits for in-flight
  streams to finish before tearing down the connection; `session.goaway(...)`
  is the lower-level primitive (send a specific error code / last-processed
  stream id / opaque debug data) — Node's own graceful-shutdown helper sends
  two `GOAWAY` frames (an initial one advertising the *actual* max stream id
  so in-flight requests are honored, then a final one) which RTS should
  replicate rather than sending a single abrupt `GOAWAY`.
- **`ping()` payload size.** Must be exactly 8 bytes if provided (a
  `PING` frame's fixed payload size); otherwise throws
  `ERR_HTTP2_PING_LENGTH`.
- **Ordering guarantees.** Within one session, frames for a given stream are
  processed in the order received; across streams, HTTP/2 explicitly
  interleaves — RTS must not assume stream N's data completes before stream
  N+1's starts. `'stream'` fires once headers are fully decompressed and
  validated, before body `DATA` frames are delivered.
- **Deprecations to track:** `http2stream.priority()` (no-op, see above);
  `request()`'s `weight` option (no-op, warns); `Http2ServerResponse.
  statusMessage` (always ignored on the wire, kept only for Compatibility-API
  signature parity with `node:http`).
- **Security — Rapid Reset (CVE-2023-44487).** A client that rapidly opens a
  stream and immediately sends `RST_STREAM` can force the server to do
  request-processing work per stream while paying near-zero cost itself,
  exhausting server resources without ever completing a request. Node's
  mitigation combines `maxSessionMemory`/`maxOutstandingPings`-style
  accounting with a reset-rate limiter that sends `GOAWAY` and closes the
  session once a client resets streams faster than a sustainable rate — RTS
  **must** implement an equivalent limiter from the start; this is not an
  optional hardening pass; see also `maxHeaderListPairs`/
  `maxDeflateDynamicTableSize` for the analogous HPACK-based exhaustion
  vectors.
- **`maxOutstandingPings`.** Bounds concurrent unacknowledged `PING`s
  (default `10`) — a basic ping-flood guard; `ping()` returns `false` rather
  than queuing once the cap is hit.
- **Performance metrics.** Node integrates `perf_hooks` `PerformanceObserver`
  entries of type `'http2'` for session/stream timing — out of scope for this
  spec; defer to `node:perf_hooks`.

---

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node` owns its full HTTP/2 stack — framing, HPACK, flow control — and
does **not** reuse `rts-std`'s `http_server` (actix-web) namespace, matching
the independence decision already applied to `node:http`.

| Area | Rust backing |
|---|---|
| Frame layer (`SETTINGS`/`HEADERS`/`DATA`/`WINDOW_UPDATE`/`PING`/`GOAWAY`/`RST_STREAM`/`PRIORITY`(inert)/`PUSH_PROMISE`/`ALTSVC`/`ORIGIN`) | Hand-rolled state machine over raw bytes, matching RFC 9113 §6 frame formats — recommended over pulling in the `h2` crate (hyperium) because several exposed Node semantics (state.weight/sumDependencyWeight pinned constants, ALTSVC/ORIGIN frames, extended CONNECT, per-frame padding strategy callback) are nghttp2-specific surface the `h2` crate does not expose 1:1; revisit if the hand-rolled path proves too costly for the P2 tier (see §7) |
| HPACK header (de)compression | Hand-rolled encoder/decoder per RFC 7541, OR the `hpack` crate if its dynamic-table-size/eviction semantics match Node's per-direction independent tables; needs static + dynamic table, Huffman coding |
| TCP transport (server accept loop, client connect, h2c) | `tokio::net::{TcpListener, TcpStream}` (same reasoning as `node:http` — concurrent multiplexed connections need async I/O, not one-thread-per-socket) |
| TLS + ALPN negotiation (`h2`/`http/1.1`) | `rustls`, vendored independently by `rts-node` (its own copy, not `rts-std`'s, per the independence decision) — ALPN protocol list `["h2", "http/1.1"]` when `allowHTTP1`, `["h2"]` otherwise |
| `respondWithFile`/`respondWithFD` | `tokio::fs` for async reads with `offset`/`length` windowing; `std::fs::Metadata` for `statCheck` |
| Settings pack/unpack (`getPackedSettings`/`getUnpackedSettings`) | pure Rust binary (de)serialization, no external crate needed |
| Rapid Reset (CVE-2023-44487) mitigation | Rust-side per-session sliding-window counter of `RST_STREAM`-shortly-after-`HEADERS` events; exceeding `streamResetRate`/`streamResetBurst` triggers `goaway` + session teardown |

### 5.2 ABI surface

Symbol convention: `__RTS_FN_NODE_HTTP2_<NAME>`. Rich stateful objects
(`Http2Server`/`Http2SecureServer`, `Http2Session` — client or server,
`Http2Stream`) are opaque `u64` Handles into `rts-node`'s own handle table
(not the engine's `gc::HandleTable`, and not shared with `rts-std`'s handle
tables — `rts-node` maintains its own slab, matching the `node:http` spec's
approach).

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_HTTP2_SERVER_CREATE` | `(StrPtr options_json)` | `Handle` | plaintext `Http2Server` |
| `__RTS_FN_NODE_HTTP2_SECURE_SERVER_CREATE` | `(StrPtr options_json /* incl. TLS + allowHTTP1 */)` | `Handle` | `Http2SecureServer` |
| `__RTS_FN_NODE_HTTP2_SERVER_LISTEN` | `(Handle, I32 port, StrPtr host, I32 backlog)` | `Void` | spawns the accept loop on the shared tokio runtime |
| `__RTS_FN_NODE_HTTP2_SERVER_CLOSE` | `(Handle)` | `Void` | |
| `__RTS_FN_NODE_HTTP2_SERVER_SET_TIMEOUT` | `(Handle, I64 msecs)` | `Void` | |
| `__RTS_FN_NODE_HTTP2_SERVER_UPDATE_SETTINGS` | `(Handle, StrPtr settings_json)` | `Void` | |
| `__RTS_FN_NODE_HTTP2_SERVER_POLL_EVENT` | `(Handle)` | `Handle` | next queued server-level event (`session`/`stream`/`request`/`sessionError`/`unknownProtocol`/…) |
| `__RTS_FN_NODE_HTTP2_CONNECT` | `(StrPtr authority, StrPtr options_json)` | `Handle` | `ClientHttp2Session`, connecting async |
| `__RTS_FN_NODE_HTTP2_SESSION_CLOSE` | `(Handle)` | `Void` | graceful (waits for in-flight streams) |
| `__RTS_FN_NODE_HTTP2_SESSION_DESTROY` | `(Handle, StrPtr err_message, I32 code)` | `Void` | |
| `__RTS_FN_NODE_HTTP2_SESSION_GOAWAY` | `(Handle, I32 code, I32 last_stream_id, Handle opaque_data_buffer)` | `Void` | |
| `__RTS_FN_NODE_HTTP2_SESSION_PING` | `(Handle, Handle payload_buffer /* 0 = generate default */)` | `Handle` | pending-ping record; resolved via `..._SESSION_POLL_EVENT` |
| `__RTS_FN_NODE_HTTP2_SESSION_SETTINGS` | `(Handle, StrPtr settings_json)` | `Void` | |
| `__RTS_FN_NODE_HTTP2_SESSION_SET_LOCAL_WINDOW_SIZE` | `(Handle, I64 window_size)` | `Void` | |
| `__RTS_FN_NODE_HTTP2_SESSION_SET_TIMEOUT` | `(Handle, I64 msecs)` | `Void` | |
| `__RTS_FN_NODE_HTTP2_SESSION_REF` / `_UNREF` | `(Handle)` | `Void` | |
| `__RTS_FN_NODE_HTTP2_SESSION_GET_STATE` | `(Handle)` | `StrPtr` | serialized `Http2SessionState` JSON |
| `__RTS_FN_NODE_HTTP2_SESSION_GET_LOCAL_SETTINGS` / `_GET_REMOTE_SETTINGS` | `(Handle)` | `StrPtr` | serialized `SettingsObject` JSON |
| `__RTS_FN_NODE_HTTP2_SESSION_POLL_EVENT` | `(Handle)` | `Handle` | next queued session event |
| `__RTS_FN_NODE_HTTP2_SESSION_ALTSVC` | `(Handle, StrPtr alt, StrPtr origin_or_stream)` | `Void` | server sessions only |
| `__RTS_FN_NODE_HTTP2_SESSION_ORIGIN` | `(Handle, StrPtr origins_json)` | `Void` | server sessions only |
| `__RTS_FN_NODE_HTTP2_REQUEST` | `(Handle session, StrPtr headers_json, StrPtr options_json)` | `Handle` | new `ClientHttp2Stream` |
| `__RTS_FN_NODE_HTTP2_STREAM_CLOSE` | `(Handle, I32 code)` | `Void` | |
| `__RTS_FN_NODE_HTTP2_STREAM_WRITE` | `(Handle, Handle chunk_buffer)` | `Bool` | backpressure signal |
| `__RTS_FN_NODE_HTTP2_STREAM_END` | `(Handle, Handle chunk_buffer)` | `Void` | |
| `__RTS_FN_NODE_HTTP2_STREAM_SEND_TRAILERS` | `(Handle, StrPtr trailers_json)` | `Void` | |
| `__RTS_FN_NODE_HTTP2_STREAM_SET_TIMEOUT` | `(Handle, I64 msecs)` | `Void` | |
| `__RTS_FN_NODE_HTTP2_STREAM_GET_STATE` | `(Handle)` | `StrPtr` | serialized `Http2StreamState` JSON |
| `__RTS_FN_NODE_HTTP2_STREAM_POLL_EVENT` | `(Handle)` | `Handle` | next queued stream event |
| `__RTS_FN_NODE_HTTP2_STREAM_READ_CHUNK` | `(Handle)` | `Handle` | next body `DATA` chunk as a Buffer handle, or an EOF sentinel |
| `__RTS_FN_NODE_HTTP2_STREAM_ADDITIONAL_HEADERS` | `(Handle, StrPtr headers_json)` | `Void` | server-only, 1xx informational |
| `__RTS_FN_NODE_HTTP2_STREAM_RESPOND` | `(Handle, StrPtr headers_json, StrPtr options_json)` | `Void` | server-only |
| `__RTS_FN_NODE_HTTP2_STREAM_RESPOND_WITH_FILE` | `(Handle, StrPtr path, StrPtr headers_json, StrPtr options_json)` | `Void` | server-only |
| `__RTS_FN_NODE_HTTP2_STREAM_RESPOND_WITH_FD` | `(Handle, I32 fd, StrPtr headers_json, StrPtr options_json)` | `Void` | server-only |
| `__RTS_FN_NODE_HTTP2_STREAM_PUSH` | `(Handle, StrPtr headers_json, StrPtr options_json)` | `Handle` | new `ServerHttp2Stream` (push); error surfaced via poll event |
| `__RTS_FN_NODE_HTTP2_GET_DEFAULT_SETTINGS` | `()` | `StrPtr` | |
| `__RTS_FN_NODE_HTTP2_GET_PACKED_SETTINGS` | `(StrPtr settings_json)` | `Handle` | Buffer handle |
| `__RTS_FN_NODE_HTTP2_GET_UNPACKED_SETTINGS` | `(Handle buffer)` | `StrPtr` | |
| `__RTS_FN_NODE_HTTP2_PERFORM_SERVER_HANDSHAKE` | `(Handle socket_handle, StrPtr options_json)` | `Handle` | `Http2Session` bound to an existing socket |
| `__RTS_FN_NODE_HTTP2_CONSTANTS_JSON` | `()` | `StrPtr` | whole `http2.constants` table, parsed/frozen once in `.ts` |

**Native-extern vs `.ts`-shim split:** framing, HPACK, flow control, TLS/ALPN,
and file responses are native externs producing/consuming opaque handles and
JSON-serialized option/header blobs. The **class shapes** (`Http2Session`/
`ServerHttp2Session`/`ClientHttp2Session`/`Http2Stream`/`ClientHttp2Stream`/
`ServerHttp2Stream`/`Http2Server`/`Http2SecureServer` as EventEmitter/Duplex
subclasses with exactly Node's method names), the **Compatibility API**
(`Http2ServerRequest`/`Http2ServerResponse` built as a thin `.ts` layer over
the Core API's `stream`/headers events, deliberately reusing the same
header-folding/option-normalization patterns as `node:http`'s shim — not
sharing code across crates, just the design), and **event-name →
EventEmitter wiring** are a `.ts` shim in `rts-node`, calling only the
externs above.

### 5.3 Async model

`node:http2` is 100% callback/event-driven at the JS surface, like
`node:http`. Internally:

- **Server accept loop + per-connection frame I/O** run on the shared
  multi-thread tokio runtime — required for concurrently multiplexing many
  streams across many connections without a thread per socket.
- **Event delivery** uses the same poll/drain model as `node:http`: Rust
  pushes typed event records into a per-handle queue
  (`Mutex<VecDeque<EventRecord>>`); the `.ts` shim's event-loop integration
  calls `..._POLL_EVENT` externs each turn and re-emits real EventEmitter
  events (`session.emit('stream', stream, headers, flags, rawHeaders)`,
  etc.), preserving the per-stream/per-session arrival ordering from §4.
- **`ping()`** resolves its callback via a queued event once the peer's `PING`
  ack frame arrives (or the request is dropped for `maxOutstandingPings`).
- **Server push (`pushStream`)** delivers its `(err, pushStream, headers)`
  callback the same way — success/failure both arrive as one queued event.
- **`connect()`/TLS handshake** completion (`'connect'` event) and ALPN
  fallback (`'unknownProtocol'`) are queued events once the async connect/
  handshake future resolves.
- **Backpressure** (`stream.write()` returning `false`, flow-control
  `WINDOW_UPDATE`): the native side tracks the session/stream's actual
  remaining flow-control window and reports it through the `Bool` return of
  `..._STREAM_WRITE`.
- **No native Promise API** — same as `node:http`, promise ergonomics are a
  userland/`util.promisify` concern layered on top of the callback surface.

### 5.4 Multithread / worker interaction

- A `Http2Server`/`Http2Session`/`Http2Stream`'s Rust-side state (frame
  buffers, HPACK dynamic tables, flow-control windows, event queues) lives in
  `rts-node`'s own handle table, guarded per-handle — never shared
  unsynchronized state.
- Per `docs/specs/rts-threading-model.md`: because HTTP/2 **multiplexes many
  logical streams over one physical connection**, the natural unit of
  thread-ownership is the **session**, not the individual stream — all
  `Http2Stream`s belonging to one `Http2Session` must be driven from that
  session's owning RTS thread; there is no meaningful notion of moving a
  single stream to another thread independently of its session.
- One `Http2Server`/`Http2Session` is used from the thread that created it;
  cross-thread use of the same handle must go through a `channel` (per the
  threading model) rather than direct concurrent extern calls on the same
  handle from two RTS threads.
- The tokio runtime is process-global/shared — many sessions from different
  RTS threads can be driven by the same worker pool safely; only the
  JS-visible handle needs single-writer discipline (same rule as
  `node:http`).
- `node:cluster`-style multi-process `SO_REUSEPORT` listen sharing and
  handing a live `Http2Session` across a `worker_threads` boundary
  (`MessagePort`-mediated handoff) are both out of scope for this spec.

### 5.5 Buffer / TypedArray interop

- **Body data** (`DATA` frames) crosses the ABI as `Buffer`/`Uint8Array`-backed
  handles (primordial TypedArray memory model) — HTTP/2 bodies are arbitrary
  bytes, exactly like `node:http`.
- **Headers** are always textual and round-tripped as `StrPtr`/JSON-serialized
  strings; HPACK (de)compression happens entirely native-side before/after
  that JSON boundary — the `.ts` layer never sees raw HPACK bytes.
- **`ping()` payload**, **`goaway()`'s `opaqueData`**, and **`getPackedSettings`
  /`getUnpackedSettings`** all move as `Buffer`-backed handles (fixed 8-byte
  payload for ping; arbitrary length for `opaqueData`; 6-bytes-per-setting for
  packed settings) — never `StrPtr`, since these are binary, not UTF-8 text.
- `stream.write()`/`respond()` body writes accept `string | Buffer` in Node;
  the `.ts` shim normalizes a `string` argument into a `Buffer` before calling
  the native `..._STREAM_WRITE` extern, keeping the extern surface
  monomorphic (same pattern as `node:http`).

### 5.6 Doctrine placement

- `http2` is **non-primordial** (reached only via `import ... from
  "node:http2"`; no native literal/syntactic form) — the engine's codegen
  never names it.
- Resolution: `node:http2` → `rts-node`'s `NodespaceSpec { node_module:
  "http2", ns_prefix: "node_http2", members: HTTP2_MEMBERS }`, registered in
  `NODE_SPECS`. A `node:http2` import resolves purely through
  `ns_prefix_for("node:http2")` → `"node_http2"` and
  `node_lookup("node_http2.connect")` → the member's `symbol`/`args`/
  `returns`, exactly like every other node module (`fs`, `path`, `http`, …)
  already registered in `rts-node`'s `lib.rs`. Zero special-casing of
  `"http2"` anywhere in `crates/rts-codegen-new/`.
- **Native-extern vs `.ts`-shim split** (restated from §5.2): framing/HPACK/
  flow-control/TLS/file-response primitives are native `extern "C"`
  functions over opaque handles and JSON option/header blobs; the full
  Node-shaped class hierarchy plus the Compatibility API's request/response
  ergonomics are a `.ts` shim shipped by `rts-node`, calling only those
  externs. No high-level API logic lives in Rust beyond raw primitives.

### 5.7 Shared-infra dependencies (FLAG)

`rts-node` cannot depend on `rts-std`, but `node:http2` needs infra that
currently lives only inside `rts-std` — the same set already flagged for
`node:http`, plus one item unique to this module:

- **Shared tokio runtime** (`rts-std::runtime::async_rt::rt()`) — the accept
  loop and per-connection frame I/O need a multi-thread async runtime; today
  private to `rts-std`. Needs a crate-neutral home, or `rts-node` stands up
  its own independent runtime instance (resource-duplication risk, flagged
  for an owner decision — same tradeoff as `node:http`).
- **Event loop pump** (`rts-std::event_loop`) — the per-turn drain of queued
  session/stream/server events into JS callbacks rides the same mechanism
  timers/promises use; if it stays `rts-std`-only, `rts-node` needs its own
  independent pump, risking two competing "tick" sources.
- **GC thread registration for tokio workers** (`gc/thread_registry` hooks
  tied to `async_rt`'s `on_thread_start`/`on_thread_stop`) — needed so the
  GC's conservative stack scanner sees handles live inside `node:http2`'s
  tokio tasks.
- **TLS backend (`rustls`)** — `node:http2`'s `createSecureServer`/`connect`
  need TLS + ALPN now (unlike `node:http`, where TLS is `node:https`'s
  concern) — per the independence decision `rts-node` vendors its **own**
  `rustls` dependency; flagging here so `node:http2` doesn't end up
  accidentally wired to `rts-std`'s copy, and so a future `node:https` +
  `node:http2` pair inside `rts-node` share **one** internal `rustls`
  configuration path (an internal-to-`rts-node` concern, not a cross-crate
  hoist).
- **HTTP/2 frame/HPACK codec** — this is new: neither `rts-std` nor
  `rts-node`'s `node:http` has an HTTP/2 codec today (`rts-std::http_server`
  is actix-web-backed and out of `rts-node`'s reach by the independence
  decision). This is not a "hoist from rts-std" item — it is a **net-new**
  dependency (hand-rolled framing + HPACK, or a vendored crate, per §5.1)
  that `rts-node` must own outright; flagged here only so it isn't mistaken
  for shared infra that already exists somewhere.

If none of the tokio/event-loop/GC items are hoisted, the fallback (as with
`node:http`) is: `rts-node` stands up its own private tokio runtime and its
own private event-queue/pump, fully isolated from `rts-std`'s — architecturally
consistent with "fully independent crate" but doubling runtime/thread overhead
for processes that use both an `rts-std`-backed feature and `node:http2`
together.

### 5.8 Implementation phases

1. **(a)** Constants (`http2.constants`, `sensitiveHeaders`) +
   `NodespaceSpec` registration (`ns_prefix = "node_http2"`) + header-name
   validation helpers (pseudo-header position rules, forbidden-header
   rejection) — zero I/O, exercises the ABI plumbing.
2. **(b)** HPACK encoder/decoder as a standalone module, tested against
   RFC 7541's published test vectors — foundational; nothing else can be
   correctness-tested without it.
3. **(c)** Minimal cleartext `h2c` Core API: connection preface, `SETTINGS`
   exchange, single-stream `HEADERS`+`DATA` round trip, server `'stream'`
   event + `stream.respond()`/`.end()`. No push, no priority frames, no
   trailers yet. Goal: `curl --http2-prior-knowledge` talks to a "hello
   world" RTS `h2c` server.
4. **(d)** TLS + ALPN (`createSecureServer`, `h2` negotiation,
   `allowHTTP1` fallback to HTTP/1.1, `'unknownProtocol'`).
5. **(e)** Compatibility API: `Http2ServerRequest`/`Http2ServerResponse` `.ts`
   shim over the Core API's `'stream'`/headers events, mirroring
   `node:http`'s `IncomingMessage`/`ServerResponse` shape.
6. **(f)** Client Core API: `connect()`, `ClientHttp2Session.request()`,
   `ClientHttp2Stream` events (`'response'`/`'headers'`/`'continue'`),
   `GOAWAY` handling on the client side.
7. **(g)** Flow control: `WINDOW_UPDATE` send/receive, `setLocalWindowSize`,
   `session.state`/`stream.state` exposure, `maxSessionMemory` accounting.
8. **(h)** Trailers: `sendTrailers`/`'wantTrailers'`/`waitForTrailers`, both
   sides.
9. **(i)** Server push: `pushStream`/`pushAllowed`/`'push'` event,
   `ERR_HTTP2_PUSH_DISABLED`/`ERR_HTTP2_NESTED_PUSH` enforcement.
10. **(j)** `ALTSVC`/`ORIGIN` frames (`session.altsvc`/`.origin`,
    `'altsvc'`/`'origin'` client events); extended `CONNECT`
    (`enableConnectProtocol`, `:protocol`) for WebSocket-over-h2 tunneling.
11. **(k)** Hardening: Rapid Reset (CVE-2023-44487) reset-rate limiter,
    `maxOutstandingPings` ping-flood guard, `maxHeaderListPairs`/
    `maxDeflateDynamicTableSize`/`maxSessionMemory` enforcement,
    `respondWithFile`/`respondWithFD` with `statCheck`/`offset`/`length`,
    `performServerHandshake` for pre-existing sockets.

---

## 6. Test plan

`tests/node/http2/*.test.ts` (using the existing `rts:test` harness/pattern).

- **Core API happy path**
  - `createServer()` (h2c) + `curl`-equivalent prior-knowledge client via
    `http2.connect({ protocol: 'http:' })`; `'stream'` fires with correct
    pseudo-headers; `stream.respond({...})` + `stream.end('body')` produces
    the exact body client-side.
  - `createSecureServer({key, cert})` + `http2.connect('https://...')`;
    `alpnProtocol === 'h2'`.
  - GET/POST/PUT/DELETE/PATCH round-trips with a body via Core API streams.
- **Compatibility API happy path**
  - `createServer(onRequestHandler)`; `req.method`/`req.headers`/`req.url`
    correct; `res.writeHead(200, {...})` + `res.end('body')` matches
    `node:http`'s observable behavior modulo `statusMessage` being ignored.
- **HPACK correctness**
  - Repeated identical headers across requests on the same session shrink on
    the wire (dynamic table reuse) — assert via a byte-counting test double,
    not timing.
  - A request exceeding `maxHeaderListPairs` is rejected (stream error, not a
    session crash).
- **Flow control**
  - Small `initialWindowSize` forces multiple `WINDOW_UPDATE` round trips
    for a large response body; assert the body arrives complete and correct
    despite windowing.
  - `setLocalWindowSize` change takes effect for subsequent transfers.
- **Trailers**
  - `waitForTrailers` + `'wantTrailers'` + `sendTrailers` round-trips on
    both client and server.
- **Server push**
  - `pushStream` succeeds when `pushAllowed`; client's `'push'` event fires
    with the promised headers; disabling push (`enablePush: false`) makes
    `pushAllowed` false and `pushStream` reject with
    `ERR_HTTP2_PUSH_DISABLED`.
  - Attempting to push from within a push stream throws
    `ERR_HTTP2_NESTED_PUSH`.
- **GOAWAY / graceful shutdown**
  - `session.close()` mid-request lets the in-flight stream finish before
    tearing down; `session.goaway(code)` with a specific error code is
    observable via the peer's `'goaway'` event with matching `errorCode`.
- **Extended CONNECT**
  - Both sides set `enableConnectProtocol: true`; a `CONNECT` request with
    `:protocol: 'websocket'` succeeds; the same request against a session
    that has not acknowledged `enableConnectProtocol` is rejected.
- **ALTSVC / ORIGIN**
  - `session.altsvc(alt, origin)` server-side is observed via the client's
    `'altsvc'` event with matching fields; `session.origin(...)` similarly
    for `'origin'`.
- **PING**
  - `session.ping(callback)` resolves with a duration and echoes the
    payload; a non-8-byte payload throws `ERR_HTTP2_PING_LENGTH`; exceeding
    `maxOutstandingPings` makes `ping()` return `false`.
- **Priority no-op parity**
  - `request(headers, { weight: 10 })` does not throw and does not affect
    behavior (matches Node's deprecated-no-op semantics); `stream.priority()`
    likewise; `stream.state.weight === 16`.
- **Security / Rapid Reset**
  - A stress test that opens many streams and immediately `RST_STREAM`s each
    one in a tight loop must trigger the reset-rate limiter (session
    `GOAWAY`+close) rather than let the server's CPU/memory usage grow
    unboundedly — regression test for CVE-2023-44487-class behavior.
  - Oversized header blocks (`maxHeaderListPairs`/`maxDeflateDynamicTableSize`
    exceeded) are rejected without unbounded allocation.
- **Errors / edge cases**
  - Requesting on a session already sent `GOAWAY` throws
    `ERR_HTTP2_GOAWAY_SESSION`.
  - Touching `session.socket.write(...)` directly throws
    `ERR_HTTP2_NO_SOCKET_MANIPULATION`.
  - `allowHTTP1: true` secure server correctly serves a plain HTTP/1.1 client
    that does not offer `h2` in ALPN.
  - `unknownProtocol` fires when ALPN negotiates neither `h2` nor
    `http/1.1` (and `allowHTTP1` is off/irrelevant).
- **Multithread**
  - Two RTS threads each create and `listen()` their own independent secure
    `Http2SecureServer` on different ports concurrently; both serve requests
    correctly with no cross-talk (validates per-thread session ownership,
    §5.4).
  - A single `Http2Session` (with several concurrently-open streams) is
    driven only from the thread that created it while another thread does
    unrelated CPU work — stress test with many concurrent streams verifying
    no cross-thread corruption of the session's internal HPACK/flow-control
    state.

---

## 7. Open questions / deferrals

- **Frame/HPACK codec choice** (hand-rolled vs. vendoring the `h2` crate) is
  not finalized — §5.1 leans hand-rolled given the nghttp2-specific surface
  Node exposes (pinned priority state, ALTSVC/ORIGIN, padding-strategy
  callback), but this should be revisited once a prototype exists; it is the
  single biggest cost driver for this P2-tier module.
- **Shared tokio runtime / event-loop pump hoisting** (§5.7) needs the same
  owner decision already pending for `node:http` — building a second,
  `rts-node`-private runtime is possible but resource-duplicating, and this
  module makes the tradeoff sharper (HTTP/2 wants many concurrent
  multiplexed streams, i.e. heavier async usage per connection than HTTP/1.1).
- **`streamResetBurst`/`streamResetRate` option names/defaults** (§3, marked
  verify) are reconstructed from general knowledge of Node's Rapid Reset
  mitigation and must be checked against the actual Node 25
  `lib/internal/http2/core.js` before being hardcoded into the ABI's option
  JSON schema.
- **Full canonical `HTTP2_HEADER_*` constant list** (§2.3) was only partially
  enumerated by the fetched Node 25 docs; the table here is reconstructed
  from Node's historically stable constant set and needs a source-level
  diff against `lib/internal/http2/constants.js` before being treated as
  exhaustive.
- **RFC 9218 Extensible Priorities** (the `priority` HTTP header, distinct
  from the now-dead RFC 7540 frame-based priority) — unclear whether Node 25's
  core `http2` module surfaces this natively or leaves it entirely to
  userland header handling; not committed to in the phase plan above, flagged
  for follow-up research before phase (j).
- **`perf_hooks` `'http2'` PerformanceObserver entries** are deferred to a
  future `node:perf_hooks` spec; no hooks are planned in this module's native
  layer beyond whatever timestamps are trivially available.
- **Internal `rts-node`-wide TLS/ALPN bring-up helper** shared between
  `node:https` (once specced) and `node:http2` (both need `rustls` server
  setup + ALPN protocol-list construction) is a good candidate for a small
  internal shared module inside `rts-node` — not a cross-crate `rts-std`
  hoist, just noted here so it isn't duplicated three times.
- **Tier ordering**: `node:http2`'s Compatibility API intentionally mirrors
  `node:http`'s object model; implementation should not start in earnest
  before `node:http` (P0)'s phases (a)-(c) have landed, so the Compatibility
  API shim can reuse proven design patterns (not code) rather than inventing
  its header-folding/option-normalization approach independently.
- **`node:https`** (out of scope here) will subclass `http.Agent`
  server-side-equivalent TLS setup that overlaps with this module's ALPN
  logic; sequencing between the two specs is an open scheduling question,
  not a technical blocker.

---
