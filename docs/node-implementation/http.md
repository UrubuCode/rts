# node:http

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:http` |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P0 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import http from "node:http"`; `import { createServer, request, get, Agent, Server, ServerResponse, IncomingMessage, ClientRequest, OutgoingMessage, METHODS, STATUS_CODES, globalAgent, maxHeaderSize } from "node:http"` |
| Globals exposed | None (all surface is module-scoped; `http.globalAgent` is a module-level singleton, not a JS global) |

---

## 1. Purpose

`node:http` provides the HTTP/1.1 client and server primitives that every
higher-level Node HTTP framework (Express, Koa, undici's HTTP/1 fallback, etc.)
is built on. It exposes a server (`http.Server`, created via
`http.createServer`) that emits a `request` event per HTTP transaction, a
client (`http.request`/`http.get`, producing `http.ClientRequest`), and the
streaming message types (`IncomingMessage`, `ServerResponse`,
`OutgoingMessage`) that carry headers and body data as Node streams. It is
deliberately low-level: no routing, no body parsing, no compression — callers
build on top of it. RTS must reproduce its object model (EventEmitter-based
classes, Duplex/Readable/Writable stream semantics) even though RTS's own
native HTTP server (`rts:http_server`, actix-web-backed) already exists as a
separate, non-Node-shaped namespace.

---

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `class Agent`
Extends: `Object` (not an `EventEmitter` in Node's http.Agent — it is a plain
class with pooling state; `https.Agent` subclasses it for TLS).

**Constructor**
```ts
new Agent(options?: AgentOptions)
```

**Instance properties**
| Property | Type | Notes |
|---|---|---|
| `freeSockets` | `Record<string, Socket[]>` | read-only; sockets awaiting reuse, keyed by `getName()` |
| `requests` | `Record<string, IncomingMessage[]>` | read-only; queued requests awaiting a socket |
| `sockets` | `Record<string, Socket[]>` | read-only; sockets currently in use |
| `maxFreeSockets` | `number` | mutable after construction |
| `maxSockets` | `number` | mutable after construction |
| `maxTotalSockets` | `number` | mutable after construction |

**Instance methods**
| Method | Signature | Returns |
|---|---|---|
| `createConnection` | `createConnection(options: object, callback?: (err: Error \| null, stream: stream.Duplex) => void): stream.Duplex` | `stream.Duplex` |
| `keepSocketAlive` | `keepSocketAlive(socket: stream.Duplex): boolean` | `boolean` |
| `reuseSocket` | `reuseSocket(socket: stream.Duplex, request: ClientRequest): void` | `void` |
| `destroy` | `destroy(): void` | `void` |
| `getName` | `getName(options?: object): string` | `string` |

**Events:** none.

---

#### `class ClientRequest`
Extends: `OutgoingMessage` (→ `stream.Writable`).
Created internally by `http.request()` / `http.get()`; never constructed directly by user code.

**Instance properties**
| Property | Type | Notes |
|---|---|---|
| `aborted` | `boolean` | **deprecated** — use `destroyed` |
| `connection` | `stream.Duplex` | **deprecated** — alias of `socket` |
| `destroyed` | `boolean` | read-only |
| `finished` | `boolean` | **deprecated** — use `writableEnded` |
| `maxHeadersCount` | `number` | default `2000` |
| `path` | `string` | read-only |
| `method` | `string` | read-only |
| `host` | `string` | read-only |
| `protocol` | `string` | read-only |
| `reusedSocket` | `boolean` | read-only; `true` when sent over a keep-alive socket |
| `socket` | `stream.Duplex` | read-only alias of `.socket` on the underlying connection |
| `writableEnded` | `boolean` | read-only |
| `writableFinished` | `boolean` | read-only |

**Instance methods**
| Method | Signature | Returns |
|---|---|---|
| `abort` | `abort(): void` | **deprecated**, use `destroy()` |
| `cork` | `cork(): void` | `void` |
| `end` | `end(data?: string \| Buffer, encoding?: BufferEncoding, callback?: () => void): this` | `this` |
| `destroy` | `destroy(error?: Error): this` | `this` |
| `flushHeaders` | `flushHeaders(): void` | `void` |
| `getHeader` | `getHeader(name: string): string \| string[] \| number \| undefined` | value |
| `getHeaderNames` | `getHeaderNames(): string[]` | `string[]` |
| `getHeaders` | `getHeaders(): Record<string, string \| string[] \| number>` | `Object` |
| `getRawHeaderNames` | `getRawHeaderNames(): string[]` | `string[]` |
| `hasHeader` | `hasHeader(name: string): boolean` | `boolean` |
| `removeHeader` | `removeHeader(name: string): void` | `void` |
| `setHeader` | `setHeader(name: string, value: string \| number \| readonly string[]): this` | `this` |
| `setNoDelay` | `setNoDelay(noDelay?: boolean): void` | `void` |
| `setSocketKeepAlive` | `setSocketKeepAlive(enable?: boolean, initialDelay?: number): void` | `void` |
| `setTimeout` | `setTimeout(timeout: number, callback?: () => void): this` | `this` |
| `uncork` | `uncork(): void` | `void` |
| `write` | `write(chunk: string \| Buffer, encoding?: BufferEncoding, callback?: (err?: Error) => void): boolean` | `boolean` |

**Events**
| Event | Callback | Notes |
|---|---|---|
| `'abort'` | `() => void` | **deprecated** |
| `'close'` | `() => void` | request completed or connection terminated |
| `'connect'` | `(response: IncomingMessage, socket: stream.Duplex, head: Buffer) => void` | server answered `CONNECT` |
| `'continue'` | `() => void` | server sent `100 Continue` |
| `'finish'` | `() => void` | request fully sent to the OS |
| `'information'` | `(info: InformationEvent) => void` | 1xx response other than upgrade |
| `'response'` | `(response: IncomingMessage) => void` | full response headers received |
| `'socket'` | `(socket: stream.Duplex) => void` | socket assigned |
| `'timeout'` | `() => void` | idle socket timeout |
| `'upgrade'` | `(response: IncomingMessage, socket: stream.Duplex, head: Buffer) => void` | server accepted protocol upgrade |

---

#### `class Server`
Extends: `net.Server` (→ `EventEmitter`).

**Constructor**
```ts
http.createServer(options？: ServerOptions, requestListener?: RequestListener): Server
new http.Server(options?: ServerOptions, requestListener?: RequestListener)   // equivalent, rarely used directly
```

**Instance properties**
| Property | Type | Default | Notes |
|---|---|---|---|
| `headersTimeout` | `number` | `min(requestTimeout, 60000)` | ms to receive complete headers |
| `listening` | `boolean` | — | read-only, inherited semantics from `net.Server` |
| `maxHeadersCount` | `number` | `2000` | per-request header count cap |
| `requestTimeout` | `number` | `300000` | ms for the whole request |
| `maxRequestsPerSocket` | `number` | `0` (unlimited) | keep-alive request cap per socket |
| `timeout` | `number` | `0` (no timeout) | socket inactivity timeout |
| `keepAliveTimeout` | `number` | `5000` | ms server waits for additional data after finishing a response |
| `keepAliveTimeoutBuffer` | `number` | `1000` | grace buffer added atop `keepAliveTimeout` before force-close |

**Instance methods**
| Method | Signature | Returns |
|---|---|---|
| `close` | `close(callback?: (err?: Error) => void): this` | `this` |
| `closeAllConnections` | `closeAllConnections(): void` | `void` |
| `closeIdleConnections` | `closeIdleConnections(): void` | `void` |
| `listen` | inherited from `net.Server` (`listen(port?, host?, backlog?, callback?)` + overloads) | `this` |
| `setTimeout` | `setTimeout(msecs?: number, callback?: () => void): this` | `this` |
| `[Symbol.asyncDispose]` | `(): Promise<void>` | disposes via `close()` |

**Events**
| Event | Callback | Notes |
|---|---|---|
| `'checkContinue'` | `(req: IncomingMessage, res: ServerResponse) => void` | `Expect: 100-continue`; if unhandled, auto `100 Continue` |
| `'checkExpectation'` | `(req: IncomingMessage, res: ServerResponse) => void` | `Expect` header other than 100-continue |
| `'clientError'` | `(exception: Error & {bytesParsed?: number, rawPacket?: Buffer}, socket: stream.Duplex) => void` | malformed request / socket error before parsing completes |
| `'close'` | `() => void` | server stopped accepting new connections |
| `'connect'` | `(req: IncomingMessage, socket: stream.Duplex, head: Buffer) => void` | `CONNECT` method |
| `'connection'` | `(socket: stream.Duplex) => void` | new TCP connection established |
| `'dropRequest'` | `(req: IncomingMessage, socket: stream.Duplex) => void` | request dropped, `maxRequestsPerSocket` exceeded |
| `'request'` | `(req: IncomingMessage, res: ServerResponse) => void` | primary handler event |
| `'upgrade'` | `(req: IncomingMessage, socket: stream.Duplex, head: Buffer) => void` | client sent `Upgrade` header and server did not otherwise respond |

---

#### `class ServerResponse`
Extends: `OutgoingMessage` (→ `stream.Writable`).
Constructed internally, passed as the 2nd arg of the `'request'` listener.

**Instance properties**
| Property | Type | Notes |
|---|---|---|
| `connection` | `stream.Duplex` | **deprecated**, alias of `socket` |
| `finished` | `boolean` | read-only, **deprecated** — use `writableEnded` |
| `headersSent` | `boolean` | read-only |
| `req` | `IncomingMessage` | read-only, the originating request |
| `sendDate` | `boolean` | default `true`; auto `Date` header |
| `socket` | `stream.Duplex` | read-only |
| `statusCode` | `number` | default `200`, writable until headers sent |
| `statusMessage` | `string` | writable until headers sent |
| `strictContentLength` | `boolean` | default `false`; throws on `Content-Length` mismatch when `true` |
| `writableEnded` | `boolean` | read-only |
| `writableFinished` | `boolean` | read-only |

**Instance methods**
| Method | Signature | Returns |
|---|---|---|
| `addTrailers` | `addTrailers(headers: Record<string, string> \| Iterable<[string,string]>): void` | `void` |
| `cork` | `cork(): void` | `void` |
| `end` | `end(data?: string \| Buffer, encoding?: BufferEncoding, callback?: () => void): this` | `this` |
| `flushHeaders` | `flushHeaders(): void` | `void` |
| `getHeader` | `getHeader(name: string): string \| string[] \| number \| undefined` | value |
| `getHeaderNames` | `getHeaderNames(): string[]` | `string[]` |
| `getHeaders` | `getHeaders(): Record<string, string \| string[] \| number>` | `Object` |
| `hasHeader` | `hasHeader(name: string): boolean` | `boolean` |
| `removeHeader` | `removeHeader(name: string): void` | `void` |
| `setHeader` | `setHeader(name: string, value: string \| number \| readonly string[]): this` | `this` |
| `setTimeout` | `setTimeout(msecs: number, callback?: () => void): this` | `this` |
| `uncork` | `uncork(): void` | `void` |
| `write` | `write(chunk: string \| Buffer, encoding?: BufferEncoding, callback?: (err?: Error) => void): boolean` | `boolean` |
| `writeContinue` | `writeContinue(): void` | `void` |
| `writeEarlyHints` | `writeEarlyHints(hints: Record<string,string \| string[]>, callback?: () => void): void` | `void` |
| `writeHead` | `writeHead(statusCode: number, statusMessage?: string, headers?: OutgoingHttpHeaders \| readonly [string,string][]): this` <br> `writeHead(statusCode: number, headers?: OutgoingHttpHeaders \| readonly [string,string][]): this` | `this` |
| `writeProcessing` | `writeProcessing(): void` | `void` |

**Events**
| Event | Callback |
|---|---|
| `'close'` | `() => void` |
| `'finish'` | `() => void` |

---

#### `class IncomingMessage`
Extends: `stream.Readable`.
Constructed internally — server-side for each request, client-side for each response.

**Instance properties**
| Property | Type | Notes |
|---|---|---|
| `aborted` | `boolean` | read-only |
| `complete` | `boolean` | read-only; `true` once fully received (incl. chunked terminator) |
| `connection` | `stream.Duplex` | read-only, alias of `socket` |
| `headers` | `Record<string, string \| string[] \| undefined>` | read-only; folded/lower-cased per Node header-join rules |
| `headersDistinct` | `Record<string, string[]>` | read-only; every header value as an array, never joined |
| `httpVersion` | `string` | read-only, e.g. `"1.1"` |
| `httpVersionMajor` | `number` | read-only |
| `httpVersionMinor` | `number` | read-only |
| `method` | `string \| undefined` | read-only; server-side only |
| `rawHeaders` | `string[]` | read-only; flat `[name, value, name, value, ...]`, original case, no folding |
| `rawTrailers` | `string[]` | read-only |
| `socket` | `stream.Duplex` | read-only |
| `statusCode` | `number \| undefined` | read-only; client-side only |
| `statusMessage` | `string \| undefined` | read-only; client-side only |
| `trailers` | `Record<string,string>` | read-only; populated only after `'end'` if trailers were sent |
| `trailersDistinct` | `Record<string,string[]>` | read-only |
| `url` | `string \| undefined` | read-only; server-side only, request-target as sent |

**Instance methods**
| Method | Signature | Returns |
|---|---|---|
| `destroy` | `destroy(error?: Error): this` | `this` |
| `setTimeout` | `setTimeout(msecs: number, callback?: () => void): this` | `this` |

**Events**
| Event | Callback |
|---|---|
| `'aborted'` | `() => void` |
| `'close'` | `() => void` |
(plus inherited `stream.Readable` events: `'data'`, `'end'`, `'error'`, `'pause'`, `'readable'`, `'resume'`.)

---

#### `class OutgoingMessage`
Extends: `stream.Writable`. Abstract base for `ClientRequest` and `ServerResponse`; never instantiated directly by user code.

**Instance properties**
| Property | Type | Notes |
|---|---|---|
| `connection` | `stream.Duplex` | read-only, **deprecated** alias of `socket` |
| `headersSent` | `boolean` | read-only |
| `socket` | `stream.Duplex` | read-only |
| `writableCorked` | `number` | read-only, cork depth |
| `writableEnded` | `boolean` | read-only |
| `writableFinished` | `boolean` | read-only |
| `writableHighWaterMark` | `number` | read-only |
| `writableLength` | `number` | read-only |
| `writableObjectMode` | `boolean` | read-only, always `false` |

**Instance methods**
| Method | Signature | Returns |
|---|---|---|
| `addTrailers` | `addTrailers(headers: Record<string,string>): void` | `void` |
| `appendHeader` | `appendHeader(name: string, value: string \| readonly string[]): this` | `this` |
| `cork` | `cork(): void` | `void` |
| `destroy` | `destroy(error?: Error): this` | `this` |
| `end` | `end(chunk?: string \| Buffer, encoding?: BufferEncoding, callback?: () => void): this` | `this` |
| `flushHeaders` | `flushHeaders(): void` | `void` |
| `getHeader` | `getHeader(name: string): string \| string[] \| number \| undefined` | value |
| `getHeaderNames` | `getHeaderNames(): string[]` | `string[]` |
| `getHeaders` | `getHeaders(): Record<string,string \| string[] \| number>` | `Object` |
| `hasHeader` | `hasHeader(name: string): boolean` | `boolean` |
| `pipe` | `pipe(): never` | throws — `OutgoingMessage` is not readable/pipeable as a source |
| `removeHeader` | `removeHeader(name: string): void` | `void` |
| `setHeader` | `setHeader(name: string, value: string \| number \| readonly string[]): this` | `this` |
| `setHeaders` | `setHeaders(headers: Headers \| Map<string, string \| string[]>): this` | `this` |
| `setTimeout` | `setTimeout(msecs: number, callback?: () => void): this` | `this` |
| `uncork` | `uncork(): void` | `void` |
| `write` | `write(chunk: string \| Buffer, encoding?: BufferEncoding, callback?: (err?: Error) => void): boolean` | `boolean` |

**Events**
| Event | Callback |
|---|---|
| `'drain'` | `() => void` |
| `'finish'` | `() => void` |
| `'prefinish'` | `() => void` |

---

### 2.2 Top-level functions

#### `createServer`
```ts
http.createServer(requestListener?: RequestListener): Server
http.createServer(options: ServerOptions, requestListener?: RequestListener): Server
```
| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `ServerOptions` | yes | `{}` |
| `requestListener` | `(req: IncomingMessage, res: ServerResponse) => void` | yes | none — attach later via `server.on('request', ...)` |

Returns: `Server` (new, not yet listening — call `.listen(...)`).
Throws: none synchronously; malformed option values throw `TypeError` (`ERR_INVALID_ARG_TYPE`, `ERR_OUT_OF_RANGE`).
Variant: **sync constructor** (the returned server's I/O is callback/event-driven).

#### `request`
```ts
http.request(options: RequestOptions | string | URL, callback?: (res: IncomingMessage) => void): ClientRequest
http.request(url: string | URL, options: RequestOptions, callback?: (res: IncomingMessage) => void): ClientRequest
```
| Param | Type | Optional | Default |
|---|---|---|---|
| `url` | `string \| URL` | yes (one of `url`/`options.host+path` required) | — |
| `options` | `RequestOptions` | yes | `{ method: 'GET', path: '/', ... }` |
| `callback` | `(res: IncomingMessage) => void` | yes | none — attach via `.on('response', ...)` |

Returns: `ClientRequest`, not yet sent (headers buffered until `.end()`/`.flushHeaders()`/first `.write()`).
Throws: `TypeError` for invalid `options.method`/`protocol` (`ERR_INVALID_HTTP_TOKEN`, `ERR_INVALID_PROTOCOL`); DNS/connect errors surface async via `'error'`.
Variant: **callback** (response delivered as an event/callback; the request itself is a stream you write to).

#### `get`
```ts
http.get(options: RequestOptions | string | URL, callback?: (res: IncomingMessage) => void): ClientRequest
http.get(url: string | URL, options: RequestOptions, callback?: (res: IncomingMessage) => void): ClientRequest
```
Same params as `request`; identical except it forces `method: 'GET'` and calls `req.end()` for you.
Returns: `ClientRequest`.
Variant: **callback**.

#### `validateHeaderName`
```ts
http.validateHeaderName(name: string, label?: string): void
```
| Param | Type | Optional | Default |
|---|---|---|---|
| `name` | `string` | no | — |
| `label` | `string` | yes | `"Header name"` |

Returns: `void`. Throws: `TypeError [ERR_INVALID_HTTP_TOKEN]` if `name` is not a valid HTTP token.
Variant: **sync**.

#### `validateHeaderValue`
```ts
http.validateHeaderValue(name: string, value: string): void
```
Returns: `void`. Throws: `TypeError [ERR_HTTP_INVALID_HEADER_VALUE]` if `value` is `undefined`; `TypeError [ERR_INVALID_CHAR]` if it contains an invalid character.
Variant: **sync**.

#### `setMaxIdleHTTPParsers`
```ts
http.setMaxIdleHTTPParsers(max: number): void
```
Sets the max number of idle (pooled) HTTP parsers retained internally. Returns `void`. Variant: **sync**.

#### `setGlobalProxyFromEnv`
```ts
http.setGlobalProxyFromEnv(proxyEnv?: ProxyEnv): void
```
Loads `HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY` (and lowercase variants) from `proxyEnv` (default `process.env`) into `http.globalAgent`'s proxy configuration. Returns `void`. Variant: **sync**.

### 2.3 Properties & constants

| Name | Type | Notes |
|---|---|---|
| `http.METHODS` | `string[]` | `['ACL','BIND','CHECKOUT','CONNECT','COPY','DELETE','GET','HEAD','LINK','LOCK','M-SEARCH','MERGE','MKACTIVITY','MKCALENDAR','MKCOL','MOVE','NOTIFY','OPTIONS','PATCH','POST','PROPFIND','PROPPATCH','PURGE','PUT','REBIND','REPORT','SEARCH','SOURCE','SUBSCRIBE','TRACE','UNBIND','UNLINK','UNLOCK','UNSUBSCRIBE']` (sorted, from llhttp) |
| `http.STATUS_CODES` | `Record<number,string>` | e.g. `{200: 'OK', 404: 'Not Found', 500: 'Internal Server Error', ...}` — full IANA-registered reason phrases |
| `http.globalAgent` | `Agent` | mutable module singleton; default agent for `request`/`get` when `options.agent` is unset |
| `http.maxHeaderSize` | `number` | default `16384` (16 KiB); mirrors `--max-http-header-size` CLI flag |

### 2.4 Events

Events are emitted per-instance (see class tables above), not at module scope. Full inventory for cross-reference:

| Class | Events |
|---|---|
| `Agent` | *(none)* |
| `ClientRequest` | `abort`(deprecated), `close`, `connect`, `continue`, `finish`, `information`, `response`, `socket`, `timeout`, `upgrade` |
| `Server` | `checkContinue`, `checkExpectation`, `clientError`, `close`, `connect`, `connection`, `dropRequest`, `request`, `upgrade` |
| `ServerResponse` | `close`, `finish` |
| `IncomingMessage` | `aborted`, `close` (+ inherited Readable events) |
| `OutgoingMessage` | `drain`, `finish`, `prefinish` (+ inherited Writable events) |

---

## 3. Types & option objects

```ts
type BufferEncoding =
  | "ascii" | "utf8" | "utf-8" | "utf16le" | "utf-16le" | "ucs2" | "ucs-2"
  | "base64" | "base64url" | "latin1" | "binary" | "hex";

interface AgentOptions {
  keepAlive?: boolean;                 // default false
  keepAliveMsecs?: number;             // default 1000
  agentKeepAliveTimeoutBuffer?: number;// default 1000
  maxSockets?: number;                 // default Infinity
  maxTotalSockets?: number;            // default Infinity
  maxFreeSockets?: number;             // default 256
  scheduling?: "fifo" | "lifo";        // default "lifo"
  timeout?: number;                    // socket idle timeout, ms
  proxyEnv?: {
    HTTP_PROXY?: string; HTTPS_PROXY?: string; NO_PROXY?: string;
    http_proxy?: string;  https_proxy?: string;  no_proxy?: string;
  };
  defaultPort?: number;                // default 80
  protocol?: string;                   // default "http:"
}

interface RequestOptions {
  agent?: Agent | boolean;             // default http.globalAgent; false = ad-hoc Agent, no pooling
  auth?: string;                       // "user:password" -> Authorization: Basic ...
  createConnection?: (options: object, callback: (err: Error | null, socket: NodeJS.Duplex) => void) => NodeJS.Duplex;
  defaultPort?: number;                // default 80
  family?: 4 | 6;                      // DNS lookup family hint
  headers?: Record<string, string | string[] | number>;
  hints?: number;                      // dns.lookup() hint flags
  host?: string;                       // default "localhost"
  hostname?: string;                   // preferred over host; wins if both set (parity with url.parse)
  insecureHTTPParser?: boolean;        // default false
  ipv6Only?: boolean;                  // default false
  joinDuplexPair?: (socket1: NodeJS.Duplex, socket2: NodeJS.Duplex) => NodeJS.Duplex;
  localAddress?: string;
  localPort?: number;
  lookup?: (hostname: string, options: object, cb: (err: Error | null, address: string, family: number) => void) => void;
  maxHeaderSize?: number;              // default 16384
  method?: string;                     // default "GET"
  path?: string;                       // default "/"; must include query string if any
  port?: number | string;              // default 80
  protocol?: string;                   // default "http:"
  setDefaultHeaders?: boolean;         // default true
  setHost?: boolean;                   // default true; auto-set Host header
  signal?: AbortSignal;
  socketPath?: string;                 // Unix domain socket path
  timeout?: number;                    // socket idle timeout applied after connect, ms
  uniqueHeaders?: (string | string[])[];
}

interface ServerOptions {
  IncomingMessage?: typeof IncomingMessage;   // custom subclass factory
  ServerResponse?: typeof ServerResponse;     // custom subclass factory
  connectionsCheckingInterval?: number;       // default 30000
  headersTimeout?: number;                    // default min(requestTimeout, 60000)
  highWaterMark?: number;                     // default 16384; stream internal buffer size
  insecureHTTPParser?: boolean;                // default false
  joinDuplexPair?: (socket1: NodeJS.Duplex, socket2: NodeJS.Duplex) => NodeJS.Duplex;
  keepAlive?: boolean;                         // default false; SO_KEEPALIVE on accepted sockets
  keepAliveInitialDelay?: number;              // default 60000 (also seen as keepAliveTimeout for HTTP keep-alive semantics)
  keepAliveTimeout?: number;                   // default 5000; HTTP keep-alive idle window
  maxHeaderSize?: number;                      // default 16384
  noDelay?: boolean;                           // default true; TCP_NODELAY
  requestTimeout?: number;                     // default 300000
  requireHostHeader?: boolean;                 // default true
  shouldUpgradeCallback?: (req: IncomingMessage) => boolean; // v24.9.0+
  uniqueHeaders?: (string | string[])[];
}

type RequestListener = (req: IncomingMessage, res: ServerResponse) => void;

interface OutgoingHttpHeaders {
  [name: string]: string | number | readonly string[] | undefined;
}

interface InformationEvent {
  httpVersion: string;
  httpVersionMajor: number;
  httpVersionMinor: number;
  statusCode: number;
  statusMessage: string;
  headers: Record<string, string | string[]>;
  rawHeaders: string[];
}

interface ClientErrorException extends Error {
  bytesParsed?: number;
  rawPacket?: Buffer;
  code?: string;   // e.g. "HPE_HEADER_OVERFLOW", "ECONNRESET"
}

type ProxyEnv = AgentOptions["proxyEnv"];
```

---

## 4. Node semantics & edge cases

- **Header folding.** `IncomingMessage.headers` lower-cases names and joins
  duplicate values per RFC rules: `set-cookie` becomes an array (never
  joined), most others are joined with `", "`, a few (`age`, `authorization`,
  `content-length`, `content-type`, `etag`, `expires`, `from`, `host`,
  `if-modified-since`, `if-unmodified-since`, `last-modified`, `location`,
  `max-forwards`, `proxy-authorization`, `referer`, `retry-after`,
  `server`, `user-agent`) keep only the **first** occurrence.
  `headersDistinct`/`rawHeaders` preserve everything, unfolded.
- **Encoding.** Headers are always latin1/ASCII on the wire (Node throws on
  invalid header-value chars, see `validateHeaderValue`); the body is raw
  bytes (`Buffer`) unless `.setEncoding()` is called on the stream — RTS
  should mirror this (never silently assume UTF-8 for the body).
  `insecureHTTPParser: true` relaxes token/whitespace validation to accept
  buggy peers — RTS must gate this behind an explicit boolean, default off.
- **`100-continue` protocol.** If the server has `'checkContinue'` listeners,
  it must decide; otherwise the default behavior is auto-`100 Continue`. On
  the client, `expectContinue` (via `Expect: 100-continue` header) delays body
  send until the `'continue'` event or a timeout — Node's timeout is 1 second
  if unspecified, no config knob is exposed for it at the public API level.
- **Chunked transfer & `Content-Length`.** `OutgoingMessage` chooses
  chunked encoding automatically when no `Content-Length` is set and the
  body is not `end()`-ed with the full payload in one call.
  `strictContentLength` (on `ServerResponse`) throws `ERR_HTTP_CONTENT_LENGTH_MISMATCH`
  if the written byte count disagrees with a manually set `Content-Length`.
- **Keep-alive vs. connection lifecycle.** Server-side, `keepAliveTimeout`
  governs how long an idle-but-kept-alive socket is held before the server
  closes it; `headersTimeout` and `requestTimeout` are separate, apply to
  in-flight requests, and exist specifically to mitigate Slowloris-style
  attacks. `maxRequestsPerSocket` bounds pipelined/keep-alive requests per
  connection (`0` = unlimited). Client-side, `Agent.keepAlive` controls
  whether sockets return to a free-list (`freeSockets`) instead of closing
  after a response completes; `scheduling` picks LIFO (default, better cache
  locality/low QPS) vs FIFO (better fairness at high QPS) when reusing a
  socket from the pool.
- **Backpressure.** `OutgoingMessage.write()` returns `false` when internal
  buffering exceeds `highWaterMark`; callers must wait for `'drain'` before
  writing more. `IncomingMessage` (a Readable) applies standard stream
  backpressure — a request handler that never reads the body pauses the
  underlying socket's TCP receive window once OS buffers fill.
- **Address family / DNS.** `family: 4 | 6` and `hints` forward to the
  internal `dns.lookup()`; `ipv6Only` disables the IPv4-mapped fallback.
  `localAddress`/`localPort` bind the client's outbound socket.
- **Unix domain sockets.** `socketPath` on `RequestOptions` uses a Unix
  domain socket instead of TCP — Windows has no direct equivalent; Node maps
  it to named pipes only via `net.Server.listen({path})`, not via
  `http.request`'s `socketPath` (that option is POSIX-only in practice on
  Windows, connect fails). RTS on Windows must surface a clear
  `ENOTSUP`/unsupported error rather than a hang.
- **Errors / errno mapping.** Common client errors surfacing on `'error'`:
  `ECONNREFUSED`, `ECONNRESET`, `ETIMEDOUT`, `EHOSTUNREACH`, `ENOTFOUND` (DNS),
  `EPIPE` (write after peer closed). Server `'clientError'` commonly carries
  `HPE_INVALID_METHOD`, `HPE_HEADER_OVERFLOW` (header block bigger than
  `maxHeaderSize`, default response is a raw `400 Bad Request` + close socket
  before `'request'` even fires), `ECONNRESET`.
- **Ordering guarantees.** For a single `ClientRequest`, `'socket'` fires
  before `'response'`; `'response'` fires before the response body's
  `'data'` events; `'finish'` (request body fully written) can fire before
  or after `'response'` depending on timing — Node does NOT guarantee request
  completion before response arrival (pipelining / early server replies).
  For `Server`, `'request'` fires once headers are fully parsed, before body
  data is available on the `IncomingMessage` stream.
- **Deprecations to track (DEP0XXX):** `request.abort()` (use `.destroy()`);
  `message.connection`/`response.connection` (use `.socket`); implicit
  string coercion nuances around header values (throws `ERR_INVALID_CHAR` for
  non-Latin1); `agent.createConnection` overriding without calling the
  original in some legacy pooling flows is discouraged (no formal DEP id,
  documented caution only).
- **Security.** `insecureHTTPParser` and raising `maxHeaderSize` both widen
  attack surface (request smuggling, memory abuse) — RTS should document
  this prominently and default to Node's safe defaults. `Server` should
  reject header blocks over `maxHeaderSize` before allocating unbounded
  buffers (matches Node' HPE_HEADER_OVERFLOW guard).

---

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node` is fully independent — it owns its transport, parser, and pooling
logic; it does **not** call into `rts-std`'s existing `http_server` (actix-web)
namespace, which stays a separate non-Node-shaped native-server surface.

| Area | Rust backing |
|---|---|
| TCP transport (server accept loop, client connect) | `std::net::{TcpListener, TcpStream}` wrapped for async via `tokio::net::{TcpListener, TcpStream}` (needed for concurrent keep-alive connections without one-OS-thread-per-socket) |
| Unix domain sockets (POSIX `socketPath`) | `tokio::net::UnixListener`/`UnixStream`, `#[cfg(unix)]`; Windows path returns a clear unsupported error (see §4) |
| HTTP/1.1 parsing (request line, headers, chunked/`Content-Length` framing) | `httparse` crate (headers) + a small hand-rolled chunked-body decoder, OR `hyper`'s `h1` internals if licensing/dependency weight is acceptable — default plan: `httparse` + own state machine, to avoid pulling in hyper's full stack for a "raw primitives" module |
| DNS resolution (`family`, `hints`, `lookup` override) | `tokio::net::lookup_host` (wraps OS resolver); falls back to `std::net::ToSocketAddrs` for sync-ish paths |
| TLS (future `node:https`, out of scope here but shares Agent shape) | `rustls` (not `rts-std`'s copy — `rts-node` vendors its own `rustls` dependency per the independence decision) |
| Header validation (`validateHeaderName`/`validateHeaderValue`) | pure Rust token/char-class checks, no external crate needed |
| Connection pooling (`Agent`) | Rust-side `HandleTable`-indexed pool: `Vec<(key, VecDeque<SocketHandle>)>` behind a `Mutex`, keyed by `getName()`-equivalent (host:port:localAddress:family) |
| Status line / reason phrases (`STATUS_CODES`) | static Rust table, mirrors IANA registry, exposed to `.ts` as a JSON-able constant via one extern that returns a serialized string (parsed once in `.ts`) |

### 5.2 ABI surface

Symbol convention: `__RTS_FN_NODE_HTTP_<NAME>`. Rich stateful objects
(`Server`, `Agent`, an in-flight `ClientRequest`/parsed message, and each
live socket) are opaque `u64` Handles into `rts-node`'s own handle table
(NOT the engine's `gc::HandleTable` directly — `rts-node` maintains its own
slab, exactly like `rts-std` does for its handle-based namespaces, since
`rts-node` cannot depend on `rts-std`; it may depend on `rts-engine` for the
`AbiType`/symbol-macro plumbing only, matching the doctrine's "engine +
primitives are the reachable low layer").

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_HTTP_SERVER_CREATE` | `(I64 options_flags, StrPtr /*serialized ServerOptions JSON*/)` | `Handle` | allocates a `Server` handle; option parsing done Rust-side from a JSON blob the `.ts` shim serializes (keeps the extern arity fixed regardless of how many optional fields Node adds) |
| `__RTS_FN_NODE_HTTP_SERVER_LISTEN` | `(Handle, I32 port, StrPtr host, I32 backlog)` | `Void` | binds + spawns the accept loop on the shared tokio runtime; errors delivered async via the event/callback bridge (§5.3), not a return code |
| `__RTS_FN_NODE_HTTP_SERVER_CLOSE` | `(Handle, Bool close_all_connections)` | `Void` | |
| `__RTS_FN_NODE_HTTP_SERVER_CLOSE_IDLE` | `(Handle)` | `Void` | |
| `__RTS_FN_NODE_HTTP_SERVER_SET_TIMEOUT` | `(Handle, I64 msecs)` | `Void` | |
| `__RTS_FN_NODE_HTTP_SERVER_POLL_EVENT` | `(Handle)` | `Handle` | pulls the next queued server-level event (`request`/`connection`/`clientError`/…) as an opaque event-record handle; drained by the `.ts` event-loop pump (see §5.3) |
| `__RTS_FN_NODE_HTTP_REQUEST_NEW` | `(StrPtr /*serialized RequestOptions JSON*/)` | `Handle` | creates a `ClientRequest`, not yet connected |
| `__RTS_FN_NODE_HTTP_REQUEST_WRITE` | `(Handle, StrPtr chunk_ptr_len /*raw bytes*/)` | `Bool` | returns backpressure signal (mirrors `.write()`'s `boolean`) |
| `__RTS_FN_NODE_HTTP_REQUEST_END` | `(Handle, StrPtr chunk)` | `Void` | flushes + finalizes; `chunk` may be empty |
| `__RTS_FN_NODE_HTTP_REQUEST_ABORT` | `(Handle)` | `Void` | maps to `.destroy()` |
| `__RTS_FN_NODE_HTTP_REQUEST_POLL_EVENT` | `(Handle)` | `Handle` | next queued client-request event (`response`/`socket`/`upgrade`/…) |
| `__RTS_FN_NODE_HTTP_MSG_HEADERS_GET` | `(Handle msg)` | `StrPtr` | serialized headers (JSON) — parsed once in `.ts` into the folded/distinct views |
| `__RTS_FN_NODE_HTTP_MSG_RAW_HEADERS_GET` | `(Handle msg)` | `StrPtr` | serialized flat raw-header array |
| `__RTS_FN_NODE_HTTP_MSG_READ_CHUNK` | `(Handle msg)` | `Handle` | pulls next body chunk as a `Buffer`/`Uint8Array`-backed handle (or a sentinel EOF handle) — body streaming rides the primordial TypedArray memory model, not `StrPtr` (bodies are arbitrary bytes, not guaranteed UTF-8) |
| `__RTS_FN_NODE_HTTP_MSG_STATUS_GET` | `(Handle msg)` | `I32` | status code (client-side `IncomingMessage`) |
| `__RTS_FN_NODE_HTTP_MSG_METHOD_GET` | `(Handle msg)` | `StrPtr` | method (server-side `IncomingMessage`) |
| `__RTS_FN_NODE_HTTP_MSG_URL_GET` | `(Handle msg)` | `StrPtr` | request-target (server-side) |
| `__RTS_FN_NODE_HTTP_MSG_HTTPVERSION_GET` | `(Handle msg)` | `StrPtr` | `"1.1"`/`"1.0"` |
| `__RTS_FN_NODE_HTTP_MSG_DESTROY` | `(Handle msg, StrPtr err_message)` | `Void` | |
| `__RTS_FN_NODE_HTTP_RES_WRITE_HEAD` | `(Handle res, I32 status, StrPtr status_message, StrPtr headers_json)` | `Void` | |
| `__RTS_FN_NODE_HTTP_RES_WRITE` | `(Handle res, Handle chunk_buffer)` | `Bool` | body chunk as a Buffer/TypedArray handle, not `StrPtr` |
| `__RTS_FN_NODE_HTTP_RES_END` | `(Handle res, Handle chunk_buffer)` | `Void` | |
| `__RTS_FN_NODE_HTTP_RES_ADD_TRAILERS` | `(Handle res, StrPtr trailers_json)` | `Void` | |
| `__RTS_FN_NODE_HTTP_RES_SET_HEADER` / `GET_HEADER` / `HAS_HEADER` / `REMOVE_HEADER` | `(Handle, StrPtr name[, StrPtr value])` | `Void`/`StrPtr`/`Bool`/`Void` | mirrored identically for `OutgoingMessage` on the request side (`REQ_SET_HEADER` etc.) |
| `__RTS_FN_NODE_HTTP_AGENT_NEW` | `(StrPtr options_json)` | `Handle` | |
| `__RTS_FN_NODE_HTTP_AGENT_DESTROY` | `(Handle)` | `Void` | |
| `__RTS_FN_NODE_HTTP_AGENT_GET_NAME` | `(Handle, StrPtr options_json)` | `StrPtr` | |
| `__RTS_FN_NODE_HTTP_VALIDATE_HEADER_NAME` | `(StrPtr name, StrPtr label)` | `Bool` | returns validity; `.ts` shim throws `TypeError` with the right code when `false` |
| `__RTS_FN_NODE_HTTP_VALIDATE_HEADER_VALUE` | `(StrPtr name, StrPtr value)` | `I32` | `0` ok, negative = which error class (undefined vs invalid char) |
| `__RTS_FN_NODE_HTTP_SET_MAX_IDLE_PARSERS` | `(I32 max)` | `Void` | |
| `__RTS_FN_NODE_HTTP_STATUS_CODES_JSON` | `()` | `StrPtr` | returns the full status-code table once; `.ts` parses + freezes into `http.STATUS_CODES` |
| `__RTS_FN_NODE_HTTP_METHODS_JSON` | `()` | `StrPtr` | returns `http.METHODS` array as JSON |

**Native-extern vs `.ts`-shim split:** everything that touches sockets,
parsing, or timers is a native extern. The **class shapes**
(`Server`/`Agent`/`ClientRequest`/`ServerResponse`/`IncomingMessage`/
`OutgoingMessage` as EventEmitter/stream subclasses with the exact method
names Node exposes), **option-object defaulting/validation that doesn't need
raw bytes**, **header folding rules** (`headers` vs `headersDistinct`
construction from the raw JSON), and **the event-name → EventEmitter
wiring** are a `.ts` shim in `rts-node`'s TS surface, calling only the
externs above. This keeps the extern surface small and stable even as
Node's option surface grows.

### 5.3 Async model

`node:http` is 100% callback/event-driven at the JS surface (no native
promise API — `util.promisify` or a user wrapping it is how promises
appear, same as real Node). Internally:

- **Server accept loop + per-connection read/write** run on the shared
  multi-thread tokio runtime (needed — blocking `std::net::TcpListener::accept`
  per-connection would burn an OS thread per keep-alive socket, unacceptable
  under load).
- **Event delivery to JS** uses a poll/drain model matching the rest of
  RTS's async surface: Rust pushes typed event records into a per-handle
  queue (`Mutex<VecDeque<EventRecord>>`); the `.ts` shim's event loop
  integration calls `..._POLL_EVENT` externs on each turn of the RTS event
  loop and re-emits them as real EventEmitter events (`server.emit('request',
  req, res)` etc.), preserving Node's ordering guarantees from §4 by queuing
  in arrival order per-handle.
- **Client requests** (`http.request`/`http.get`): connect + write happen on
  a tokio task; `'response'`/`'data'`/`'end'` events are queued the same way.
- **Backpressure** (`write()` returning `false`, `'drain'`): the native side
  tracks the tokio socket's actual write-readiness and reports it through
  the `Bool` return of `..._WRITE`; `'drain'` is queued as an event once the
  socket becomes writable again.
- **Promise subsystem interaction:** none directly required by `node:http`
  itself (it is not promise-based), but the underlying primitives it shares
  with the rest of RTS (timers for `keepAliveTimeout`/`requestTimeout`,
  the event-loop pump) are the same shared infra other promise-based modules
  use — see §5.7 for what must be hoisted.

### 5.4 Multithread / worker interaction

- A `Server`/`Agent`/in-flight request's Rust-side state (socket handles,
  pooled-connection lists, event queues) lives in `rts-node`'s own handle
  table, guarded by `Mutex`/lock-free sharding analogous to the engine's
  `HandleTable` sharding — **not** shared unsynchronized state.
- Per the RTS threading model (`docs/specs/rts-threading-model.md`):
  a `Server` handle created on one RTS thread is conceptually owned by
  that thread's region; if a `worker_threads.Worker` needs to hand off a
  listening socket (Node's `cluster` module semantics — out of scope for
  this spec, tracked separately under `node:cluster`), that requires
  **promotion to the shared heap** plus an explicit hand-off, not implicit
  sharing. For this spec (`node:http` alone, no `cluster`), the practical
  rule is: **one `Server`/`Agent` is used from the thread that created it**;
  cross-thread use of the same handle must go through a `channel` (per the
  threading model) rather than direct concurrent extern calls on the same
  handle from two RTS threads.
- The tokio runtime itself is process-global/shared (per `rts-threading-model.md`
  and the existing `async_rt` precedent) — many `Server`/`ClientRequest`
  handles from different RTS threads can be driven by the same worker pool
  safely; only the **JS-visible handle** needs single-writer discipline.
- `SharedArrayBuffer`-backed request/response bodies are out of scope for
  v1 (bodies are copied through owned `Buffer`/`Uint8Array` handles); revisit
  if zero-copy shared-heap bodies become a measured bottleneck.

### 5.5 Buffer / TypedArray interop

- Request/response **bodies** cross the ABI as `Buffer`/`Uint8Array`-backed
  handles (primordial TypedArray memory model), never as `StrPtr` — HTTP
  bodies are arbitrary bytes, and Node's stream API always deals in
  `Buffer` chunks unless `.setEncoding()` is used (which is a `.ts`-side
  decode step over the same underlying bytes, not a different wire path).
- **Headers** are always textual (Latin-1/ASCII per RFC 7230) and are
  round-tripped as `StrPtr`/JSON-serialized strings — no TypedArray
  involvement.
- `ServerResponse.write`/`ClientRequest.write` accept `string | Buffer` in
  Node; the `.ts` shim normalizes a `string` argument (with its encoding) into
  a `Buffer` *before* calling the native `..._WRITE` extern, so the extern
  only ever sees bytes — keeps the ABI surface monomorphic.
- `IncomingMessage`'s Readable side surfaces chunks as `Buffer` by default
  (or `string` post `setEncoding`); `..._MSG_READ_CHUNK` returns a Buffer
  handle per chunk, consistent with the above.

### 5.6 Doctrine placement

- `http` is **non-primordial** (no native literal/syntactic form — it is
  reached via `import ... from "node:http"` / `require("node:http")`, a
  plain module namespace, not something the engine's codegen ever names).
- Resolution: `node:http` → `rts-node`'s `NodespaceSpec { node_module: "http",
  ns_prefix: "node_http", members: HTTP_MEMBERS }`, registered in
  `NODE_SPECS`. The engine/codegen never hardcodes `"http"` anywhere; a
  `node:http` import resolves purely through `ns_prefix_for("node:http")` →
  `"node_http"` and `node_lookup("node_http.request")` → the member's
  `symbol`/`args`/`returns`, exactly like every other node module and
  exactly like the existing `fs`/`path`/`os`/`process`/`util`/`crypto`
  modules already registered in `rts-node`'s `lib.rs`. Zero special-casing
  of `"http"` in `crates/rts-codegen-new/`.
- **Native-extern vs `.ts`-shim split** (restated from §5.2): the transport,
  parser, pooling, and header-validation primitives are native `extern "C"`
  functions producing/consuming opaque handles and JSON-serialized option
  blobs; the full Node-shaped class hierarchy (`Server extends net.Server`,
  `ClientRequest`/`ServerResponse` extends `OutgoingMessage`,
  `IncomingMessage extends stream.Readable`, EventEmitter wiring, the
  `writeHead` overload resolution, header-folding presentation) is a `.ts`
  shim shipped by `rts-node`, calling only the externs in §5.2. This is the
  same split already used for `fs`/`process` etc. — no high-level API logic
  lives in Rust beyond raw primitives.

### 5.7 Shared-infra dependencies (FLAG)

`rts-node` cannot depend on `rts-std`, but `node:http` needs infra that
currently lives only inside `rts-std`. These must be hoisted into a shared
low crate (`rts-engine`, or a new small shared crate both `rts-primitives`-
tier and `rts-node` can depend on) before this module can be built for real:

- **Shared tokio runtime** (`rts-std::runtime::async_rt::rt()`) — the
  server accept loop and client connect/read/write need a multi-thread async
  runtime; today it is a `rts-std`-private `OnceLock<Runtime>`. Needs a
  crate-neutral home (or `rts-node` must own a second independent tokio
  runtime instance, which risks thread-count/resource duplication and is
  the less-preferred option — flagging for an owner decision).
- **Event loop pump** (`rts-std::event_loop`) — the mechanism that drains
  per-handle event queues each turn and re-enters JS callbacks. `node:http`'s
  event delivery model (§5.3) rides on the same pump used by timers/promise
  settlement; if it stays `rts-std`-only, `rts-node` needs its own
  independent pump wired into the same top-level run loop, which risks two
  competing "tick" sources — flagging for consolidation.
- **GC thread registration for tokio workers** (`gc/thread_registry`
  registration hooks tied to `async_rt`'s `on_thread_start`/`on_thread_stop`)
  — needed so the GC's conservative stack scanner sees handles live inside
  `node:http`'s tokio tasks; currently coupled to `rts-std`'s runtime
  bring-up code.
- **TLS backend (`rustls`)** — not required for `node:http` itself, but
  `node:https`'s `Agent` subclasses `http.Agent` and will need this; flagging
  now so the `http`/`https` split doesn't end up with two independent
  `rustls` configurations. Per the independence decision, `rts-node` vendors
  its **own** `rustls` dependency rather than reusing `rts-std`'s — noted
  here so it isn't accidentally wired to the `rts-std` copy later.
- **DNS resolution helper** — `rts-std::net` has existing `dns.resolve`-style
  code; `rts-node` should NOT reuse those externs (independence decision),
  but the underlying `tokio::net::lookup_host` usage pattern can be
  duplicated freely (it is a stdlib/tokio call, not `rts-std` code) — noted
  for completeness, not a hard blocker.

If none of the above is hoisted, the fallback is: `rts-node` stands up its
own private tokio runtime + its own private event-queue/pump, fully isolated
from `rts-std`'s. That is architecturally consistent with "fully
independent crate" but doubles runtime/thread overhead for programs that use
both an `rts-std`-backed feature (e.g. `rts:http_server`) and `node:http` in
the same process — worth an explicit owner call before implementation
starts.

### 5.8 Implementation phases

1. **(a)** Header validation + `STATUS_CODES`/`METHODS` constants + the
   `NodespaceSpec` registration boilerplate (`ns_prefix = "node_http"`) with
   zero I/O — smallest possible vertical slice, exercises the ABI plumbing.
2. **(b)** `OutgoingMessage`/`IncomingMessage` `.ts` shims with header
   getter/setter/folding logic tested against static fixtures (no sockets
   yet) — unblocks unit-testing header semantics in isolation.
3. **(c)** Minimal `Server` + `ServerResponse`: `createServer` →
   `listen` → accept loop on tokio → parse one request with `httparse` →
   emit `'request'` → `res.writeHead`/`res.end` → close. No keep-alive, no
   chunked body, no trailers yet. Gets `curl` talking to a "hello world"
   RTS HTTP server.
4. **(d)** Request/response **body streaming**: chunked transfer-encoding
   decode/encode, `Content-Length` framing, backpressure (`write()` boolean +
   `'drain'`), `IncomingMessage` as a real Readable with `'data'`/`'end'`.
5. **(e)** Keep-alive: `keepAliveTimeout`, `headersTimeout`, `requestTimeout`,
   `maxRequestsPerSocket`, socket reuse on the server side.
6. **(f)** Client side: `http.request`/`http.get`, `ClientRequest` write/end,
   `'response'`/`'socket'`/`'timeout'` events, error mapping (`ECONNREFUSED`
   etc.).
7. **(g)** `Agent` pooling: `keepAlive`, `maxSockets`/`maxFreeSockets`/
   `maxTotalSockets`, `scheduling` (`lifo`/`fifo`), `getName`,
   `http.globalAgent` singleton, `agent: false` bypass.
8. **(h)** `100-continue` protocol (`'checkContinue'`, `writeContinue`,
   client `'continue'`/`expectContinue`), `'checkExpectation'`.
9. **(i)** `CONNECT`/`'connect'` event + `Upgrade`/`'upgrade'` event (both
   server and client sides), `shouldUpgradeCallback`.
10. **(j)** Trailers (`addTrailers`, `rawTrailers`/`trailersDistinct`),
    `writeEarlyHints` (103 Early Hints), `writeProcessing` (102), `AbortSignal`
    wiring (`options.signal`), `Symbol.asyncDispose` on `Server`.
11. **(k)** Edge/security hardening: `insecureHTTPParser`, `maxHeaderSize`
    enforcement + `HPE_HEADER_OVERFLOW`-equivalent `'clientError'`,
    `uniqueHeaders`, IPv6/`family`/`hints`/`ipv6Only`, Unix domain sockets
    (POSIX) + explicit unsupported-on-Windows error for `socketPath` on
    `http.request`.

---

## 6. Test plan

`tests/node/http/*.test.ts` (using the existing `rts:test` harness/pattern).

- **Server happy path**
  - `createServer` + `listen(0)` (ephemeral port) + `request` event fires
    with correct `method`/`url`/`headers`; `res.writeHead(200, {...})` +
    `res.end('body')` produces a client-observable 200 with the exact body.
  - GET / POST / PUT / DELETE / PATCH round-trips with a body.
  - Multiple headers with the same name fold correctly (`set-cookie` stays
    an array; a joinable header joins with `, `).
  - `rawHeaders` preserves original case and order.
- **Client happy path**
  - `http.request(url, cb)` and `http.request(options, cb)` both connect and
    receive a response; `http.get` auto-ends the request.
  - Streaming a large body via multiple `.write()` calls, verifying
    backpressure (`write()` returns `false` under a small `highWaterMark`,
    `'drain'` fires before more writes).
- **Keep-alive**
  - Two sequential requests over the same `Agent({keepAlive: true})` reuse
    the same underlying socket (assert via a server-side connection counter).
  - Server `keepAliveTimeout` closes an idle kept-alive socket after the
    configured window; verify via timing assertion with generous tolerance.
  - `maxRequestsPerSocket` triggers `'dropRequest'` on the Nth+1 request.
- **Chunked / trailers**
  - Response without `Content-Length` streams chunked; client reassembles
    the full body correctly.
  - `addTrailers` + a `Trailer` header round-trip; client reads
    `trailers`/`rawTrailers` after `'end'`.
- **100-continue**
  - Client sends `Expect: 100-continue`; server without a `'checkContinue'`
    listener auto-responds `100 Continue` and receives the body.
  - Server with `'checkContinue'` listener that calls
    `res.writeHead(417)` instead — client never sends the body.
- **Upgrade / CONNECT**
  - Client sends `Connection: Upgrade, Upgrade: websocket`; server emits
    `'upgrade'` with the raw socket and `head` buffer; verify byte-exact
    hand-off (no dropped/duplicated bytes across the transition).
  - `CONNECT` method emits `'connect'` server-side and `'connect'`
    client-side symmetrically.
- **Errors / edge cases**
  - Connecting to a closed port yields `'error'` with `ECONNREFUSED`.
  - Malformed request line / oversized headers (> `maxHeaderSize`) triggers
    `'clientError'` and the server sends a raw 400 and closes — verify no
    `'request'` event fired for that connection.
  - `res.end()` called twice does not throw and does not double-send.
  - Calling `res.setHeader()` after `res.writeHead()`/first `res.write()`
    throws (`ERR_HTTP_HEADERS_SENT`-equivalent).
  - `req.destroy(new Error("boom"))` mid-request surfaces on `'error'`/`'close'`
    both client- and server-side.
  - `AbortSignal` passed via `options.signal`, aborted before response
    arrives — request destroys with an `AbortError`-equivalent.
  - Header value with an invalid character throws via
    `http.validateHeaderValue`/`setHeader`.
- **Constants**
  - `http.METHODS` contains the expected sorted list; `http.STATUS_CODES[404]
    === 'Not Found'`.
- **Multithread**
  - Two RTS threads each create and `listen()` their own independent
    `Server` on different ports concurrently; both serve requests correctly
    with no cross-talk (validates per-thread handle isolation, §5.4).
  - A single `Server` handle is created on the main thread and only ever
    driven from that thread while a worker thread does unrelated CPU work
    concurrently — verifies no implicit cross-thread mutation of the
    handle's internal queue causes corruption (stress test with many
    concurrent client connections while the worker thread runs).

---

## 7. Open questions / deferrals

- **HTTP parser choice** (`httparse` + hand-rolled chunked/state-machine vs.
  vendoring `hyper`'s h1 codec) is not finalized — flagged in §5.1; affects
  how much of chunked-encoding/pipelining correctness comes "for free".
- **Shared tokio runtime / event-loop pump hoisting** (§5.7) needs an owner
  decision before implementation of phases (c)+ can start for real — building
  a second, `rts-node`-private runtime is possible but resource-duplicating.
- **`node:cluster` / SO_REUSEPORT multi-process listen sharing** is
  explicitly out of scope for this spec; the §5.4 threading-model mapping
  covers only same-process multithreaded use of `node:http`.
- **HTTP/2 / `node:http2`** is a separate module/spec; `node:http`'s `Agent`
  shape is the base `https.Agent` will subclass, but TLS (`node:https`) and
  h2 are deferred to their own specs.
- **`WebSocket` global** mentioned tangentially by Node's docs (the
  `ws`-compatible `WebSocket` client class added as a global in recent Node)
  is unrelated to `node:http`'s exports and is out of scope for this spec.
- **Windows `socketPath` client behavior**: Node's own behavior here is
  murky/undocumented for named pipes via `http.request`; RTS's plan (explicit
  unsupported error) should be revisited if real-world Windows named-pipe
  HTTP clients turn out to be a common ask.
- **Precise timing of the default `100-continue` timeout** (~1100ms in
  Node's actual implementation, not officially documented as a stable
  contract) is treated as an implementation detail, not a tested contract,
  in the §6 test plan (assertions use generous tolerances).
- **`proxyEnv`/`setGlobalProxyFromEnv`** (proxy support in `Agent`, Node
  v24.5+) full HTTP/HTTPS proxy tunneling behavior (CONNECT-based tunneling
  through a proxy for HTTPS targets) is documented in outline only here;
  detailed proxy-tunneling semantics should get a follow-up pass once basic
  client/server phases (f)-(g) land.
