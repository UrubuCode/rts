# node:https

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:https` |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P1 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import https from "node:https"`; `import { createServer, request, get, Agent, Server, globalAgent } from "node:https"` |
| Globals exposed | None (all surface is module-scoped; `https.globalAgent` is a module-level singleton, not a JS global) |

---

## 1. Purpose

`node:https` is `node:http` with a TLS transport layer underneath: it exposes
the same request/response object model (`IncomingMessage`, `ServerResponse`,
`ClientRequest`, `OutgoingMessage`) and largely the same `Agent`/`Server`
surface, but connections are secured via TLS 1.2/1.3 (certificate
verification, SNI, ALPN, session resumption). `https.Server` extends
`tls.Server` (adding HTTP framing on top of a secure socket) and
`https.Agent` extends `http.Agent` (adding TLS connection options + session
caching for client requests). It is the module every `fetch()`/`undici`-like
HTTPS client and every production-facing Node HTTP server built without a
reverse proxy is layered on. RTS must reproduce both the HTTP object model
(shared with `node:http`) and the TLS semantics (certificate validation
defaults, SNI, ALPN, session cache, `keylog`) faithfully, while keeping
`rts-node` fully independent of `rts-std`'s existing `tls`/`http_server`
native namespaces.

---

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `class Agent`
Extends: `http.Agent`.

**Constructor**
```ts
new Agent(options?: HttpsAgentOptions)
```

**Instance properties** (own, beyond inherited `http.Agent` properties)
| Property | Type | Notes |
|---|---|---|
| `maxCachedSessions` | `number` | default `100`; TLS session cache size (`0` disables session resumption) |
| `servername` | `string` | default: request's host name, or `''` for an IP-address host; SNI value the agent will send when the request doesn't override it |

Inherited from `http.Agent`: `freeSockets`, `requests`, `sockets`,
`maxFreeSockets`, `maxSockets`, `maxTotalSockets` (see `node:http` spec).

**Instance methods**
| Method | Signature | Returns |
|---|---|---|
| `createConnection` | `createConnection(options: object, callback?: (err: Error \| null, stream: tls.TLSSocket) => void): tls.TLSSocket` | overridden to establish a `tls.connect()` instead of a plain `net.connect()`; overridable further by user subclasses |
| (all other `http.Agent` methods) | `keepSocketAlive`, `reuseSocket`, `destroy`, `getName` | inherited unchanged (see `node:http` spec) |

**Events**
| Event | Callback | Notes |
|---|---|---|
| `'keylog'` | `(line: Buffer, tlsSocket: tls.TLSSocket) => void` | emitted once per new TLS key-material line, for **any** socket created by this agent; `line` is in NSS `SSLKEYLOGFILE` format — security-sensitive, see §4 |

---

#### `class Server`
Extends: `tls.Server` (→ `net.Server` → `EventEmitter`). Adds the HTTP
request/response framing that `http.Server` provides, layered on top of the
secured socket (i.e. functionally `Server` behaves like `http.Server` +
`tls.Server` combined — Node implements this via internal delegation rather
than multiple inheritance, but the full method/event surface of both is
present).

**Constructor**
```ts
https.createServer(options?: HttpsServerOptions, requestListener?: RequestListener): Server
new https.Server(options?: HttpsServerOptions, requestListener?: RequestListener)   // equivalent, rarely used directly
```

**Instance properties** (own + inherited)
| Property | Type | Default | Notes |
|---|---|---|---|
| `headersTimeout` | `number` | `min(requestTimeout, 60000)` | same semantics as `http.Server` |
| `maxHeadersCount` | `number` | `2000` | same as `http.Server` |
| `requestTimeout` | `number` | `300000` | same as `http.Server` |
| `timeout` | `number` | `0` (no timeout) | same as `http.Server` |
| `keepAliveTimeout` | `number` | `5000` | same as `http.Server` |
| `maxConnections` | `number` | — | inherited from `tls.Server`/`net.Server`; `Infinity` if unset |
| `connections` | `number` | — | **deprecated**, inherited from `net.Server`; use `getConnections()` |

**Instance methods** (own + inherited)
| Method | Signature | Returns |
|---|---|---|
| `close` | `close(callback?: (err?: Error) => void): this` | `this` |
| `closeAllConnections` | `closeAllConnections(): void` | `void` |
| `closeIdleConnections` | `closeIdleConnections(): void` | `void` |
| `listen` | inherited from `net.Server` (`listen(port?, host?, backlog?, callback?)` + overloads) | `this` |
| `setTimeout` | `setTimeout(msecs?: number, callback?: () => void): this` | `this` |
| `[Symbol.asyncDispose]` | `(): Promise<void>` | disposes via `close()` |
| `addContext` | `addContext(hostname: string, context: tls.SecureContextOptions \| tls.SecureContext): void` | inherited from `tls.Server`; adds a per-SNI-hostname secure context |
| `setSecureContext` | `setSecureContext(options: tls.SecureContextOptions): void` | inherited from `tls.Server`; replaces the server-wide default secure context (does not affect existing connections) |
| `getTicketKeys` | `getTicketKeys(): Buffer` | inherited from `tls.Server` |
| `setTicketKeys` | `setTicketKeys(keys: Buffer): void` | inherited from `tls.Server` |

**Events** (own HTTP-shaped + inherited `tls.Server`)
| Event | Callback | Notes |
|---|---|---|
| `'checkContinue'` | `(req: IncomingMessage, res: ServerResponse) => void` | same as `http.Server` |
| `'checkExpectation'` | `(req: IncomingMessage, res: ServerResponse) => void` | same as `http.Server` |
| `'clientError'` | `(exception: Error & {bytesParsed?: number, rawPacket?: Buffer}, socket: tls.TLSSocket) => void` | same as `http.Server`, socket is a `TLSSocket` |
| `'close'` | `() => void` | server stopped accepting new connections |
| `'connect'` | `(req: IncomingMessage, socket: tls.TLSSocket, head: Buffer) => void` | `CONNECT` method |
| `'connection'` | `(socket: tls.TLSSocket) => void` | new secure connection established (note: this fires **after** the TLS handshake completes, unlike `tls.Server`'s raw `'connection'` semantics on plain sockets) |
| `'dropRequest'` | `(req: IncomingMessage, socket: tls.TLSSocket) => void` | `maxRequestsPerSocket`-equivalent guard |
| `'request'` | `(req: IncomingMessage, res: ServerResponse) => void` | primary handler event |
| `'upgrade'` | `(req: IncomingMessage, socket: tls.TLSSocket, head: Buffer) => void` | client sent `Upgrade` header |
| `'keylog'` | `(line: Buffer, tlsSocket: tls.TLSSocket) => void` | inherited from `tls.Server`; key material for every accepted connection |
| `'newSession'` | `(sessionId: Buffer, sessionData: Buffer, callback: () => void) => void` | inherited from `tls.Server`; custom session-cache store hook |
| `'OCSPRequest'` | `(certificate: Buffer, issuer: Buffer, callback: (err: Error \| null, resp?: Buffer) => void) => void` | inherited from `tls.Server`; OCSP stapling hook |
| `'resumeSession'` | `(sessionId: Buffer, callback: (err: Error \| null, sessionData: Buffer \| null) => void) => void` | inherited from `tls.Server`; custom session-cache lookup hook |
| `'secureConnection'` | `(tlsSocket: tls.TLSSocket) => void` | inherited from `tls.Server`; fires once the TLS handshake completes, before HTTP parsing begins |
| `'tlsClientError'` | `(exception: Error, tlsSocket: tls.TLSSocket) => void` | inherited from `tls.Server`; handshake-level failure (before `'secureConnection'`) |

---

### 2.2 Top-level functions

#### `createServer`
```ts
https.createServer(requestListener?: RequestListener): Server
https.createServer(options: HttpsServerOptions, requestListener?: RequestListener): Server
```
| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `HttpsServerOptions` | yes (nominally — a *working* server needs `cert`+`key`, `pfx`, or `secureContext`; omitting all of them makes the server unable to complete a handshake) | `{}` |
| `requestListener` | `(req: IncomingMessage, res: ServerResponse) => void` | yes | none — attach later via `server.on('request', ...)` |

Returns: `Server` (new, not yet listening — call `.listen(...)`).
Throws: `Error`/`TypeError` synchronously for structurally invalid TLS
options (e.g. malformed PEM in `cert`/`key`, mismatched key/cert pair
detected at context-build time); DNS/bind errors surface async via
`'error'`.
Variant: **sync constructor** (I/O is callback/event-driven).

#### `request`
```ts
https.request(options: HttpsRequestOptions | string | URL, callback?: (res: IncomingMessage) => void): ClientRequest
https.request(url: string | URL, options?: HttpsRequestOptions, callback?: (res: IncomingMessage) => void): ClientRequest
```
| Param | Type | Optional | Default |
|---|---|---|---|
| `url` | `string \| URL` | yes (one of `url`/`options.host+path` required) | — |
| `options` | `HttpsRequestOptions` | yes | `{ method: 'GET', path: '/', protocol: 'https:', port: 443, agent: https.globalAgent }` |
| `callback` | `(res: IncomingMessage) => void` | yes | none — attach via `.on('response', ...)` |

Returns: `ClientRequest`, not yet sent (headers buffered until
`.end()`/`.flushHeaders()`/first `.write()`; TLS handshake itself is
performed lazily on first byte of traffic, same as `net.connect`+`tls.connect`
composition).
Throws: `TypeError` for invalid `options.method`/`protocol`
(`ERR_INVALID_HTTP_TOKEN`, `ERR_INVALID_PROTOCOL`); certificate
verification failures, DNS/connect errors, and handshake failures all
surface async via `'error'` on the request (never a synchronous throw).
Variant: **callback**.

#### `get`
```ts
https.get(options: HttpsRequestOptions | string | URL, callback?: (res: IncomingMessage) => void): ClientRequest
https.get(url: string | URL, options?: HttpsRequestOptions, callback?: (res: IncomingMessage) => void): ClientRequest
```
Same params as `request`; identical except it forces `method: 'GET'` and
calls `req.end()` for you.
Returns: `ClientRequest`.
Variant: **callback**.

### 2.3 Properties & constants

| Name | Type | Notes |
|---|---|---|
| `https.globalAgent` | `Agent` | mutable module singleton; default agent for `request`/`get` when `options.agent` is unset. Diverges from a plain `new https.Agent()`: `keepAlive: true` and `timeout: 5000` (since v19.0.0) |

`https` has no `METHODS`/`STATUS_CODES`/`maxHeaderSize`-equivalent exports of
its own — those live on `node:http` and are not re-exported here (a `.ts`
consumer that needs them imports `node:http` directly, matching real Node).

### 2.4 Events

Events are emitted per-instance (see class tables above), not at module
scope. Full inventory for cross-reference:

| Class | Events |
|---|---|
| `Agent` | `keylog` |
| `Server` | `checkContinue`, `checkExpectation`, `clientError`, `close`, `connect`, `connection`, `dropRequest`, `request`, `upgrade` (HTTP-shaped, same as `http.Server`) **+** `keylog`, `newSession`, `OCSPRequest`, `resumeSession`, `secureConnection`, `tlsClientError` (TLS-shaped, from `tls.Server`) |
| `ClientRequest` / `IncomingMessage` / `ServerResponse` / `OutgoingMessage` | identical to `node:http` — see that spec; the underlying socket exposed via `'socket'` is a `tls.TLSSocket` instead of a plain `net.Socket`, adding TLS-only events reachable via `req.socket.on(...)`: `'secureConnect'`, `'session'`, `'OCSPResponse'`, `'keylog'` (client-side, per-socket) |

---

## 3. Types & option objects

```ts
type BufferEncoding =
  | "ascii" | "utf8" | "utf-8" | "utf16le" | "utf-16le" | "ucs2" | "ucs-2"
  | "base64" | "base64url" | "latin1" | "binary" | "hex";

// -- shared with node:tls / node:http, restated here for completeness --

interface SecureContextOptions {
  ca?: string | string[] | Buffer | Buffer[];             // trusted CA cert(s); default: Node's bundled root store
  cert?: string | string[] | Buffer | Buffer[];            // cert chain in PEM
  ciphers?: string;                                        // OpenSSL cipher-list format; default: Node's default cipher suite
  clientCertEngine?: string;                                // deprecated (runtime-removed upstream since OpenSSL ENGINE API removal) — see §7
  crl?: string | string[] | Buffer | Buffer[];              // PEM CRL(s)
  dhparam?: string | Buffer;                                // DHE parameters ('auto' generates well-known groups)
  ecdhCurve?: string;                                       // default 'auto'
  honorCipherOrder?: boolean;
  key?: string | string[] | Buffer | Buffer[] | { pem: string | Buffer; passphrase?: string }[];
  passphrase?: string;                                      // decrypts `key`/`pfx`
  pfx?: string | Buffer | { buf: string | Buffer; passphrase?: string }[];
  privateKeyEngine?: string;                                // deprecated — see §7
  privateKeyIdentifier?: string;                            // deprecated — see §7
  secureOptions?: number;                                   // OpenSSL SSL_OP_* bitmask
  secureProtocol?: string;                                  // legacy method name (e.g. 'TLSv1_2_method'); modern code should use minVersion/maxVersion
  sessionIdContext?: string;
  sigalgs?: string;
  ticketKeys?: Buffer;
  minVersion?: "TLSv1.3" | "TLSv1.2" | "TLSv1.1" | "TLSv1"; // default 'TLSv1.2'
  maxVersion?: "TLSv1.3" | "TLSv1.2" | "TLSv1.1" | "TLSv1"; // default 'TLSv1.3'
}

interface TlsConnectionOptions extends SecureContextOptions {
  ALPNProtocols?: string[] | Buffer | TypedArray | DataView;
  checkServerIdentity?: (servername: string, cert: PeerCertificate) => Error | undefined;
  enableTrace?: boolean;
  handshakeTimeout?: number;      // server-side, default 120000
  rejectUnauthorized?: boolean;   // default true (client); server requestCert-dependent
  requestCert?: boolean;          // server-side, default false
  requestOCSP?: boolean;          // client-side
  servername?: string;            // SNI hostname override
  session?: Buffer;               // client-side TLS session resumption blob
  SNICallback?: (servername: string, cb: (err: Error | null, ctx?: tls.SecureContext) => void) => void;
  pskCallback?: (socket: tls.TLSSocket, identity: string) => Buffer | null;
  pskIdentityHint?: string;
  highWaterMark?: number;          // default 16 * 1024
}

interface PeerCertificate {
  subject: Record<string, string>;
  issuer: Record<string, string>;
  valid_from: string;
  valid_to: string;
  serialNumber: string;
  fingerprint: string;     // SHA-1
  fingerprint256: string;  // SHA-256
  fingerprint512: string;  // SHA-512
  raw: Buffer;             // DER-encoded cert
  subjectaltname?: string;
  infoAccess?: Record<string, string[]>;
  ca?: boolean;
}

// -- https-specific composite option shapes --

interface HttpsAgentOptions extends AgentOptions, TlsConnectionOptions {
  maxCachedSessions?: number;   // default 100; 0 disables session resumption
  servername?: string;          // default: request host, or '' for IP hosts
}
// AgentOptions == the same shape documented in node:http's spec
// (keepAlive, keepAliveMsecs, maxSockets, maxFreeSockets, maxTotalSockets,
//  scheduling, timeout, proxyEnv, defaultPort default 443, protocol default 'https:')

interface HttpsServerOptions extends SecureContextOptions, TlsConnectionOptions {
  IncomingMessage?: typeof IncomingMessage;
  ServerResponse?: typeof ServerResponse;
  connectionsCheckingInterval?: number;   // default 30000
  headersTimeout?: number;                 // default min(requestTimeout, 60000)
  highWaterMark?: number;                  // default 16384
  insecureHTTPParser?: boolean;             // default false
  joinDuplexPair?: (socket1: NodeJS.Duplex, socket2: NodeJS.Duplex) => NodeJS.Duplex;
  keepAlive?: boolean;                      // default false; SO_KEEPALIVE on accepted sockets
  keepAliveInitialDelay?: number;           // default 60000
  keepAliveTimeout?: number;                // default 5000
  maxHeaderSize?: number;                   // default 16384
  noDelay?: boolean;                        // default true
  requestTimeout?: number;                  // default 300000
  requireHostHeader?: boolean;              // default true
  shouldUpgradeCallback?: (req: IncomingMessage) => boolean;
  uniqueHeaders?: (string | string[])[];
  allowHalfOpen?: boolean;                  // default false (tls.Server / net.Server option)
  pauseOnConnect?: boolean;                 // default false (net.Server option)
  sessionTimeout?: number;                  // TLS session cache lifetime, seconds; default 300
  ticketKeys?: Buffer;
}

interface HttpsRequestOptions extends TlsConnectionOptions {
  agent?: Agent | boolean;                 // default https.globalAgent; false = ad-hoc Agent
  auth?: string;                            // "user:password" -> Authorization: Basic ...
  createConnection?: (options: object, callback: (err: Error | null, socket: NodeJS.Duplex) => void) => NodeJS.Duplex;
  defaultPort?: number;                     // default 443
  family?: 4 | 6;
  headers?: Record<string, string | string[] | number>;
  hints?: number;
  host?: string;                            // default "localhost"
  hostname?: string;                        // preferred over host
  insecureHTTPParser?: boolean;              // default false
  ipv6Only?: boolean;                       // default false
  localAddress?: string;
  localPort?: number;
  lookup?: (hostname: string, options: object, cb: (err: Error | null, address: string, family: number) => void) => void;
  maxHeaderSize?: number;                    // default 16384
  method?: string;                           // default "GET"
  path?: string;                             // default "/"
  port?: number | string;                    // default 443
  protocol?: string;                         // default "https:"
  secureContext?: tls.SecureContext;         // pre-built context; if unset, one is built per-request from the TLS options above
  setDefaultHeaders?: boolean;               // default true
  setHost?: boolean;                         // default true
  signal?: AbortSignal;
  socketPath?: string;
  timeout?: number;
  uniqueHeaders?: (string | string[])[];
}

type RequestListener = (req: IncomingMessage, res: ServerResponse) => void;

interface AgentOptions {
  keepAlive?: boolean; keepAliveMsecs?: number; agentKeepAliveTimeoutBuffer?: number;
  maxSockets?: number; maxTotalSockets?: number; maxFreeSockets?: number;
  scheduling?: "fifo" | "lifo"; timeout?: number;
  proxyEnv?: { HTTP_PROXY?: string; HTTPS_PROXY?: string; NO_PROXY?: string;
               http_proxy?: string; https_proxy?: string; no_proxy?: string; };
  defaultPort?: number; protocol?: string;
}
```

---

## 4. Node semantics & edge cases

- **Default certificate verification is ON.** `rejectUnauthorized` defaults
  to `true` on the client — an invalid/expired/self-signed/hostname-mismatched
  certificate makes the request emit `'error'` and never connect. This is the
  opposite of some other ecosystems' historical defaults; RTS must not
  silently weaken it. `NODE_TLS_REJECT_UNAUTHORIZED=0` (env var) is Node's
  escape hatch that disables verification **process-wide** — security-critical,
  Node itself prints a runtime warning when set; RTS should replicate both the
  behavior and the warning if it supports the env var at all (see §7).
- **SNI (`servername`).** Defaults to the connection's hostname (or `''` for
  a literal IP, since IPs can't carry SNI per RFC 6066). `https.Agent`'s own
  `servername` option lets a caller override the default per-agent (useful
  behind a proxy/tunnel where the TCP target differs from the logical
  hostname).
- **ALPN.** `ALPNProtocols` on the client offers a protocol list (e.g.
  `['h2','http/1.1']`); the negotiated protocol is read from the resulting
  `tls.TLSSocket.alpnProtocol`. `node:https` itself only ever *speaks*
  HTTP/1.1 over the wire regardless of what ALPN negotiates — full HTTP/2 is
  `node:http2`'s job, out of scope here (see §7).
- **Session resumption.** Client-side, `Agent.maxCachedSessions` (default
  100) caches TLS session tickets keyed by the target `(host, port,
  servername, ...)` tuple; a resumed handshake skips the full asymmetric
  handshake round trip (1-RTT vs 2-RTT, meaningfully faster for repeat
  connections). Server-side, `sessionTimeout` (default 300s) bounds how long
  the server-side session cache entry lives; `'newSession'`/`'resumeSession'`
  let a user override the cache store entirely (e.g. for a shared cache
  across server processes).
- **`keylog` event.** Emits raw `SSLKEYLOGFILE`-format lines for **every**
  connection made through the `Agent`/`Server` once at least one listener is
  attached. Intended for offline decryption with tools like Wireshark during
  debugging. **Security note**: never attach a `'keylog'` listener that
  persists key material in a production environment — logging it defeats
  forward secrecy for any traffic captured alongside it.
- **Certificate chain validation vs `checkServerIdentity`.** Node splits
  verification into two phases: (1) OpenSSL-level chain-of-trust validation
  against the CA bundle (`ca` option or Node's bundled roots) — controlled by
  `rejectUnauthorized`; (2) hostname-matching against the certificate's
  `subjectAltName`/CN — controlled by `checkServerIdentity` (which runs even
  when `rejectUnauthorized: false`, but its result is only enforced when
  `rejectUnauthorized: true`; with verification off, a mismatch is reported
  via `tlsSocket.authorizationError` instead of aborting the connection).
- **Root CA bundle.** Node ships its own bundled Mozilla root CA set
  (independent of the OS trust store) unless `--use-openssl-ca` or
  `NODE_OPTIONS=--use-system-ca` (Node 24+) is used. RTS's existing
  `rts-std` TLS namespace already uses `rustls` + `webpki-roots` (embedded
  Mozilla roots) — `rts-node` should match this choice for parity and
  cross-platform consistency (see §5.1/§5.7), rather than trusting a
  platform-specific store (Windows SChannel roots vs Linux OpenSSL roots
  differ in practice and would make certificate-acceptance behavior diverge
  by OS, which is a portability trap).
- **Platform differences.** TLS itself is not POSIX/Windows-divergent at the
  protocol level when using `rustls` (pure-Rust, no OpenSSL/SChannel
  dependency) — this is a deliberate cross-platform simplification vs Node's
  OpenSSL binding, which does pick up OS-provided engines/config on some
  platforms. Where `node:https` composes with `node:http`'s
  Windows-vs-POSIX gaps (Unix domain sockets via `options.path`/`socketPath`),
  the same caveats from the `node:http` spec apply unchanged.
- **Errors / errno mapping.** TLS-specific error codes surfacing on
  `'error'`/`'tlsClientError'`: `CERT_HAS_EXPIRED`, `DEPTH_ZERO_SELF_SIGNED_CERT`,
  `UNABLE_TO_VERIFY_LEAF_SIGNATURE`, `SELF_SIGNED_CERT_IN_CHAIN`,
  `ERR_TLS_CERT_ALTNAME_INVALID` (hostname mismatch, from
  `checkServerIdentity`), `ERR_TLS_HANDSHAKE_TIMEOUT`, `ECONNRESET` (peer
  aborted mid-handshake), plus everything inherited from `node:http`'s error
  surface once the secure channel is established. Underlying OpenSSL-style
  `err.code`/`err.library`/`err.reason` triple is Node/OpenSSL-specific; RTS
  (backed by `rustls`) should map its own error taxonomy onto the same
  `err.code` strings for drop-in compatibility, documenting any code that has
  no exact `rustls` equivalent.
- **Ordering guarantees.** For a client request: TCP connect → TLS handshake
  (`'secureConnect'` on the socket) → `'socket'` event on the request → HTTP
  request bytes written → `'response'`. For the server: TCP accept →
  handshake → `'secureConnection'` (raw `tls.TLSSocket`, HTTP framing not
  parsed yet) → `'request'` (once the first request's headers are parsed off
  that socket) → subsequent keep-alive requests each re-fire `'request'`
  without a new `'secureConnection'`.
- **Deprecations / removals to track:** `clientCertEngine`/`privateKeyEngine`/
  `privateKeyIdentifier` depend on OpenSSL's ENGINE API, which upstream
  OpenSSL and therefore Node have been deprecating/removing; `rustls` has no
  ENGINE-equivalent concept at all — RTS treats these as **explicit
  non-goals** (see §7), not a gap to fill later.
- **Backpressure / streaming.** Identical to `node:http` once the secure
  channel is up — `OutgoingMessage.write()` backpressure, `IncomingMessage`
  Readable semantics, `highWaterMark` all apply unchanged on top of the
  decrypted byte stream.
- **Security notes (restated for emphasis).** Never default
  `rejectUnauthorized: false`; never log `keylog` output persistently by
  default; `minVersion` should default to `'TLSv1.2'` (matches modern Node —
  TLS 1.0/1.1 are legacy and increasingly rejected by peers/regulations);
  weak/export ciphers should not be in the default cipher list.

---

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node`'s `https` module is built **on top of** `rts-node`'s own `http`
module (in-crate composition — both live under `rts-node`, so this is not a
forbidden cross-crate dependency; it mirrors how Node itself layers
`lib/https.js` over `lib/_http_server.js`/`lib/_http_client.js`). It adds a
TLS layer underneath the same connection/parsing/pooling code `node:http`
already implements.

| Area | Rust backing |
|---|---|
| TLS handshake (client + server), record layer | `rustls` (async via the `tokio-rustls` wrapper — **`rts-node` vendors its own copy**, does not reuse `rts-std`'s existing `rustls` dependency, per the independence decision; see §5.7) |
| Root CA bundle | `webpki-roots` (embedded Mozilla root set), matching `rts-std`'s existing `tls` namespace approach — avoids OS-trust-store divergence across Windows/macOS/Linux |
| Secure context construction (`ca`/`cert`/`key`/`pfx`/`ciphers`/min-max version) | Rust-side builder over `rustls::{ClientConfig, ServerConfig}`; PEM/PKCS12 parsing via `rustls-pemfile` + a PKCS12 crate for `pfx` |
| Session resumption (client cache, server cache) | `rustls`'s built-in `ClientSessionMemoryCache` / server-side session storage, sized from `maxCachedSessions`/`sessionTimeout` |
| ALPN negotiation | `rustls::ClientConfig::alpn_protocols` / `ServerConfig::alpn_protocols`, surfaced back to `.ts` via a getter extern |
| SNI dispatch (`addContext`/`SNICallback`) | `rustls::server::ResolvesServerCert` implementation keyed by the SNI hostname map built from `addContext` calls |
| Certificate inspection (`getPeerCertificate`-equivalent) | `x509-parser` (or `rustls`'s own cert parsing primitives) to extract subject/issuer/validity/fingerprint from the DER bytes `rustls` exposes |
| HTTP/1.1 framing, header validation, Agent pooling, request/response object model | reused as-is from `rts-node`'s `node:http` implementation — https sockets are handed to the exact same parser/pool code once the handshake completes |
| `keylog` capture | `rustls`'s `KeyLog` trait implementation, feeding a per-handle line queue |

### 5.2 ABI surface

Symbol convention: `__RTS_FN_NODE_HTTPS_<NAME>`. Because HTTP framing,
headers, and body streaming are byte-for-byte identical to `node:http` once
a socket is secured, **https reuses `node:http`'s message/body/header
externs one-for-one** (`__RTS_FN_NODE_HTTP_MSG_*`, `__RTS_FN_NODE_HTTP_RES_*`,
`__RTS_FN_NODE_HTTP_REQUEST_WRITE/END`, `__RTS_FN_NODE_HTTP_VALIDATE_HEADER_*`,
etc. — see the `node:http` spec, §5.2). Only the connection-establishment
layer (context/handshake/certificate concerns) needs `_HTTPS_`-specific
externs:

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_HTTPS_SECURE_CONTEXT_NEW` | `(StrPtr /*serialized SecureContextOptions JSON*/)` | `Handle` | builds an `rustls::{ClientConfig,ServerConfig}`-backed context; one context is reusable across many connections (immutable, `Arc`-wrapped — see §5.4) |
| `__RTS_FN_NODE_HTTPS_SECURE_CONTEXT_ADD_SNI` | `(Handle ctx, StrPtr hostname, Handle sni_ctx)` | `Void` | implements `addContext`/SNI dispatch table |
| `__RTS_FN_NODE_HTTPS_SECURE_CONTEXT_DESTROY` | `(Handle)` | `Void` | |
| `__RTS_FN_NODE_HTTPS_SERVER_CREATE` | `(Handle secure_ctx, StrPtr options_json /*ServerOptions subset shared with http*/)` | `Handle` | allocates a `Server` handle; delegates the accept-loop/parsing wiring to the same internal machinery as `__RTS_FN_NODE_HTTP_SERVER_CREATE`, plus a `TlsAcceptor` |
| `__RTS_FN_NODE_HTTPS_SERVER_LISTEN` | `(Handle, I32 port, StrPtr host, I32 backlog)` | `Void` | binds + spawns accept loop; each accepted socket is handshaked before entering the shared HTTP request-parsing path |
| `__RTS_FN_NODE_HTTPS_SERVER_CLOSE` / `_CLOSE_IDLE` / `_SET_TIMEOUT` / `_POLL_EVENT` | mirrors `__RTS_FN_NODE_HTTP_SERVER_*` | — | identical shape, TLS-specific events (`secureConnection`, `tlsClientError`, `newSession`, `resumeSession`, `OCSPRequest`, `keylog`) are additional variants in the polled event record's tag enum |
| `__RTS_FN_NODE_HTTPS_AGENT_NEW` | `(StrPtr options_json /*HttpsAgentOptions incl. maxCachedSessions, servername, TLS opts*/)` | `Handle` | |
| `__RTS_FN_NODE_HTTPS_AGENT_DESTROY` | `(Handle)` | `Void` | |
| `__RTS_FN_NODE_HTTPS_REQUEST_NEW` | `(Handle agent, StrPtr options_json /*HttpsRequestOptions*/)` | `Handle` | creates a `ClientRequest`; connect + handshake deferred until first write/flush, same lazy semantics as `node:http` |
| `__RTS_FN_NODE_HTTPS_REQUEST_POLL_EVENT` | `(Handle)` | `Handle` | next queued event; adds `secureConnect`/`session`/`OCSPResponse`/`keylog` tags atop `node:http`'s `response`/`socket`/`upgrade`/… |
| `__RTS_FN_NODE_HTTPS_SOCKET_GET_PEER_CERT` | `(Handle tls_socket, Bool detailed)` | `StrPtr` | serialized `PeerCertificate` JSON (subject/issuer/valid_from/valid_to/fingerprints); `detailed=true` includes the full chain |
| `__RTS_FN_NODE_HTTPS_SOCKET_GET_PROTOCOL` | `(Handle tls_socket)` | `StrPtr` | e.g. `"TLSv1.3"` |
| `__RTS_FN_NODE_HTTPS_SOCKET_GET_ALPN_PROTOCOL` | `(Handle tls_socket)` | `StrPtr` | negotiated ALPN protocol, or empty if none |
| `__RTS_FN_NODE_HTTPS_SOCKET_GET_CIPHER` | `(Handle tls_socket)` | `StrPtr` | serialized `{name, version}` JSON |
| `__RTS_FN_NODE_HTTPS_SOCKET_AUTHORIZED` | `(Handle tls_socket)` | `Bool` | mirrors `tlsSocket.authorized` |
| `__RTS_FN_NODE_HTTPS_SOCKET_AUTH_ERROR` | `(Handle tls_socket)` | `StrPtr` | mirrors `tlsSocket.authorizationError` (empty if authorized) |
| `__RTS_FN_NODE_HTTPS_CHECK_SERVER_IDENTITY` | `(StrPtr servername, StrPtr peer_cert_json)` | `StrPtr` | built-in hostname-matching algorithm; empty string = match, non-empty = error message. The `.ts` shim calls this as the default and lets a user-supplied `checkServerIdentity` override it |
| `__RTS_FN_NODE_HTTPS_KEYLOG_POLL` | `(Handle conn)` | `StrPtr` | drains queued keylog lines for one connection handle (only populated when a `'keylog'` listener is attached — no cost otherwise) |

**Native-extern vs `.ts`-shim split:** everything that touches the TLS
handshake, certificate parsing, or session cache is a native extern
returning opaque handles/JSON blobs. The `Agent`/`Server` class shapes
(EventEmitter wiring, event-name translation, `checkServerIdentity`
override plumbing, option defaulting: `port` 443, `protocol` 'https:',
`agent` defaulting to `https.globalAgent`) are a `.ts` shim in `rts-node`,
which for the HTTP-shaped parts of `Server`/`ClientRequest`/`IncomingMessage`/
`ServerResponse` literally **re-exports and extends** the `node:http` `.ts`
shim classes rather than re-implementing them.

### 5.3 Async model

Same callback/event-driven model as `node:http` (no native promise API).
The TLS handshake adds one more inherently-async step:

- **Handshake** runs on the shared multi-thread tokio runtime via
  `tokio_rustls::{TlsAcceptor, TlsConnector}` — a blocking handshake would
  stall a whole OS thread per connection, unacceptable under load exactly
  like the plaintext accept-loop case in `node:http`.
- **Event delivery** uses the identical poll/drain model as `node:http`
  (§5.3 there): Rust pushes typed event records (now including
  `secureConnection`/`tlsClientError`/`newSession`/`resumeSession`/
  `OCSPRequest`/`keylog`/`secureConnect`/`session`) into the same per-handle
  queue; the `.ts` event-loop integration drains them each turn and
  re-emits real EventEmitter events, preserving the ordering guarantees in
  §4 (handshake completion always ordered before the first `'request'`).
- **Certificate verification** itself is synchronous CPU work
  (`rustls`'s cert-chain validation) done inline during the handshake
  future — no separate thread pool needed (unlike e.g. bcrypt-style
  password hashing); it does not block the tokio worker for a meaningfully
  long time under normal certificate-chain sizes.
- **Session cache lookups/stores** (`'newSession'`/`'resumeSession'`
  user-overridable hooks) round-trip through the same event
  queue/callback bridge — a user hook is invoked async, and the handshake
  future awaits the queued response before completing.

### 5.4 Multithread / worker interaction

- Same per-handle single-owner-thread rule as `node:http` (§5.4 there): a
  `Server`/`Agent`/in-flight `ClientRequest` handle is used from the RTS
  thread that created it; cross-thread use goes through a `channel` per the
  threading model (`docs/specs/rts-threading-model.md`), not concurrent
  extern calls on the same handle from two threads.
- **Secure contexts are the one piece of state that's cheap to share.** A
  built `rustls::{ClientConfig, ServerConfig}` is immutable and internally
  `Arc`-wrapped — many `Agent`/`Server`/`ClientRequest` handles across
  different RTS threads can safely reference the **same** underlying
  `SecureContext` handle (e.g. one process-wide default context) without
  needing shared-heap promotion, since `rustls` configs contain no
  interior-mutable per-connection state. This is worth calling out
  explicitly: unlike sockets/event-queues (single-writer), secure contexts
  are a natural multithread-shared resource and RTS's handle table should
  reflect that (reference-counted handle, not a per-thread copy).
- The shared tokio runtime (process-global, per `async_rt` precedent) drives
  handshakes/reads/writes for handles created on any RTS thread, same as
  `node:http`.
- `worker_threads`-style hand-off of a listening `https.Server` socket
  (Node's `cluster` semantics) is out of scope here, same deferral as
  `node:http`.

### 5.5 Buffer / TypedArray interop

- Identical to `node:http` (§5.5 there) for HTTP bodies once the channel is
  secured: request/response bodies cross as `Buffer`/`Uint8Array`-backed
  handles, headers as `StrPtr`/JSON.
- **Certificate material** (`cert`/`key`/`ca`/`pfx` inputs, and
  `PeerCertificate.raw`/chain bytes on output) are DER/PEM byte blobs — PEM
  inputs arrive as `string`/`Buffer` per Node's typing and cross the ABI as
  `StrPtr` (PEM is ASCII-safe text) or a `Buffer` handle (DER/PFX, which is
  binary); the `.ts` shim normalizes whichever form the caller provided
  before invoking `__RTS_FN_NODE_HTTPS_SECURE_CONTEXT_NEW`.
- `keylog` lines are ASCII text (`StrPtr`), not binary.
- `ALPNProtocols` input accepts `string[] | Buffer | TypedArray | DataView`
  in Node; the `.ts` shim normalizes all forms into a `string[]` before
  serializing into the options JSON blob (keeps the native side single-shaped).

### 5.6 Doctrine placement

- `https` is **non-primordial** (no native literal/syntactic form; reached
  only via `import ... from "node:https"`).
- Resolution: `node:https` → `rts-node`'s `NodespaceSpec { node_module:
  "https", ns_prefix: "node_https", members: HTTPS_MEMBERS }`, registered in
  `NODE_SPECS` alongside `node_http`. The engine/codegen never hardcodes
  `"https"` anywhere; resolution goes purely through
  `ns_prefix_for("node:https")` → `"node_https"` and
  `node_lookup("node_https.request")` → the member's `symbol`/`args`/
  `returns`, identical in shape to every other node module.
- **Native-extern vs `.ts`-shim split** (restated from §5.2): TLS handshake,
  context-building, certificate parsing, session-cache, and ALPN are native
  `extern "C"` functions over opaque handles/JSON blobs; the full
  Node-shaped class hierarchy (`Agent extends http.Agent`,
  `Server` with the combined `http.Server`+`tls.Server` event/method
  surface) is a `.ts` shim in `rts-node`, composing the `node:http` `.ts`
  shim rather than duplicating it. No high-level API logic lives in Rust
  beyond raw TLS/context primitives.

### 5.7 Shared-infra dependencies (FLAG)

`rts-node` cannot depend on `rts-std`. `node:https` needs everything
`node:http` already flags (restated, not duplicated infra) **plus** TLS-specific
items:

- **Shared tokio runtime** — same flag as `node:http`'s §5.7; the TLS
  handshake future runs on it too. No new item beyond what `node:http`
  already requires hoisting.
- **Event loop pump** — same flag as `node:http`'s §5.7; https's additional
  event tags ride the same pump.
- **GC thread registration for tokio workers** — same flag as `node:http`'s
  §5.7.
- **`rustls` dependency** — `node:http`'s spec already flags this as needed
  for *future* `node:https`; this spec is that future arriving. Per the
  independence decision `rts-node` vendors its **own** `rustls`, distinct
  from `rts-std`'s existing `tls` namespace copy. Flagging explicitly here
  so implementation doesn't accidentally re-wire to the `rts-std` instance.
- **Root CA bundle strategy** (NEW, https-specific) — needs an explicit
  owner decision: embed `webpki-roots` (matches `rts-std`'s existing `tls`
  namespace, fully portable, but can drift from the OS's own updated/
  enterprise-injected trust store) vs. `rustls-native-certs` (reads the OS
  store, matches enterprise MITM-proxy/custom-CA expectations but
  reintroduces Windows-vs-POSIX behavioral variance). Recommendation in this
  spec: default to `webpki-roots` for parity with `rts-std`'s prior art,
  revisit if enterprise-CA use cases demand OS-store support.
- **`NODE_TLS_REJECT_UNAUTHORIZED` env var handling** (NEW) — if RTS
  supports this compat knob at all, its behavior needs to be centralized
  (ideally in whatever shared TLS-adjacent low crate ends up hosting secure
  contexts) so `node:https` and a future `node:tls` don't implement two
  divergent readings of the same env var.

If none of the above is hoisted, the fallback (same as `node:http`'s) is:
`rts-node` stands up its own private tokio runtime + event pump + vendored
`rustls`, fully isolated from `rts-std`'s equivalents — architecturally
consistent with independence but duplicates runtime/thread/TLS-config
overhead for a process using both an `rts-std`-backed feature and
`node:https`.

### 5.8 Implementation phases

This module has a **hard implementation-order dependency**: `node:http`'s
phases (a)-(g) (its own §5.8) should land first, since `https.Agent`/
`Server`/`ClientRequest` are built by composing that code.

1. **(a)** `NodespaceSpec` registration (`ns_prefix = "node_https"`) +
   `HttpsAgentOptions`/`HttpsServerOptions`/`HttpsRequestOptions` JSON
   validation/defaulting (port 443, protocol 'https:') — zero TLS I/O yet.
2. **(b)** `SecureContext` builder: PEM/DER parsing for `cert`/`key`/`ca`,
   `rustls::{ClientConfig, ServerConfig}` construction, embedded
   `webpki-roots` default, `minVersion`/`maxVersion`/`ciphers` mapping.
3. **(c)** Minimal `https.Server`: `createServer` → `listen` → TCP accept →
   `TlsAcceptor::accept` handshake → hand off the resulting stream into the
   **existing** `node:http` request-parsing path unchanged → `'request'`
   fires. Gets a self-signed-cert "hello world" HTTPS server answering
   `curl -k`.
4. **(d)** Minimal `https.request`/`get` client: `TlsConnector::connect`
   with SNI (`servername`), default `rejectUnauthorized: true` cert-chain
   validation, `'secureConnect'`/`'error'` on failure.
5. **(e)** `checkServerIdentity` (built-in hostname matcher +
   user-override), `PeerCertificate` inspection surface
   (`getPeerCertificate`-equivalent bridging), `authorized`/
   `authorizationError`.
6. **(f)** `https.Agent`: `maxCachedSessions` session resumption,
   `servername` default, `https.globalAgent` singleton (keepAlive true,
   timeout 5000), composing `http.Agent`'s pooling.
7. **(g)** ALPN negotiation surface (`ALPNProtocols` in, `alpnProtocol` out)
   — negotiation only; no HTTP/2 speaking on top of it in this module.
8. **(h)** `keylog` event (client + server + agent), `'newSession'`/
   `'resumeSession'` custom session-store hooks, `sessionTimeout`.
9. **(i)** SNI dispatch: `addContext`/`SNICallback`, multi-hostname virtual
   servers on one `https.Server`.
10. **(j)** mTLS: `requestCert`, client-certificate presentation on the
    request side, server-side verification + `'secureConnection'` with
    `tlsSocket.authorized`.
11. **(k)** OCSP stapling (`requestOCSP` client-side, `'OCSPRequest'`
    server-side hook) — best-effort, flagged as open in §7 given `rustls`'s
    more limited OCSP surface vs OpenSSL.
12. **(l)** Security/parity hardening: `NODE_TLS_REJECT_UNAUTHORIZED`
    compat decision execution, default cipher/version alignment audit,
    `Symbol.asyncDispose` on `Server`.

---

## 6. Test plan

`tests/node/https/*.test.ts` (using the existing `rts:test` harness/pattern,
with locally generated self-signed cert/key fixtures checked in or
generated at test-setup time).

- **Server happy path**
  - `createServer({cert, key})` + `listen(0)` + client connects with the
    matching `ca` → `'request'` fires, `res.writeHead(200)` + `res.end()`
    round-trips correctly end to end.
  - `tlsSocket.getPeerCertificate()`-equivalent surface returns the expected
    subject/issuer/fingerprint for a known test cert.
- **Client certificate verification (default secure)**
  - Client connecting to a self-signed server **without** supplying `ca`
    gets `'error'` with a `DEPTH_ZERO_SELF_SIGNED_CERT`/
    `SELF_SIGNED_CERT_IN_CHAIN`-equivalent code; connection never completes.
  - Same server, client supplies the matching `ca` → connects successfully.
  - Expired test certificate → `'error'` with `CERT_HAS_EXPIRED`-equivalent.
  - `rejectUnauthorized: false` explicitly set → connects despite
    self-signed cert, but `tlsSocket.authorized === false` and
    `authorizationError` is populated (verifies the "still tells you" contract).
- **SNI**
  - `addContext('a.test', ctxA)` + `addContext('b.test', ctxB)` on one
    server; two clients connecting with different `servername` values each
    receive the matching virtual-host certificate.
- **ALPN**
  - Client offers `['h2','http/1.1']`; server configured for
    `['http/1.1']` only → negotiated protocol is `'http/1.1'` on both sides.
- **Session resumption**
  - Two sequential connections through the same `Agent({maxCachedSessions:
    10})` to the same server reuse a cached session (assert via a
    server-side session-cache-hit counter or handshake-type introspection);
    `maxCachedSessions: 0` disables reuse (assert full handshake both times).
- **`keylog`**
  - Attaching a `'keylog'` listener on the client and the server both
    receive at least one line per connection, in the expected `CLIENT_*`/
    NSS key-log line format; no listener attached → no measurable overhead
    from key-material capture (smoke-tested, not perf-asserted precisely).
- **Custom `checkServerIdentity`**
  - A custom callback that always returns `undefined` (accept) lets a
    hostname-mismatched cert through; one that returns an `Error` rejects
    even an otherwise-valid cert.
- **mTLS**
  - Server with `requestCert: true` + a client presenting a valid client
    cert → server-side `'secureConnection'` sees `tlsSocket.authorized ===
    true`; a client presenting no cert or an untrusted one is reflected in
    `authorized`/`authorizationError` per the server's `rejectUnauthorized`
    setting.
- **Errors / edge cases**
  - Connecting to a closed port yields `'error'` with `ECONNREFUSED`
    (same as plain TCP, TLS never begins).
  - Handshake against a plaintext HTTP server on the same port (protocol
    mismatch) fails cleanly with a handshake-level error, not a hang.
  - `AbortSignal` passed via `options.signal`, aborted mid-handshake →
    request destroys with an `AbortError`-equivalent.
  - Malformed PEM in `cert`/`key` at `createServer()` time throws
    synchronously (per §2.2), not lazily at first connection.
- **Multithread**
  - Two RTS threads each create and `listen()` their own independent
    `https.Server` (distinct certs) on different ports concurrently; both
    serve requests correctly with no cross-talk.
  - One `SecureContext` handle built once and referenced by `Agent`/
    `Server` instances created on **different** RTS threads (validates the
    §5.4 claim that secure contexts are safely multithread-shared without
    per-handle single-owner restrictions).

---

## 7. Open questions / deferrals

- **Root CA bundle strategy** (`webpki-roots` embedded vs
  `rustls-native-certs` OS store) is an explicit owner decision flagged in
  §5.7; this spec recommends `webpki-roots` for parity with `rts-std`'s
  prior art but does not finalize it.
- **OCSP stapling** (`requestOCSP`, `'OCSPRequest'`) is flagged best-effort
  (§5.8 phase (k)) — `rustls`'s OCSP support surface is narrower than
  OpenSSL's; exact fidelity (e.g. stapled-response validation edge cases)
  needs a dedicated investigation before claiming full parity.
- **`clientCertEngine`/`privateKeyEngine`/`privateKeyIdentifier`** (OpenSSL
  ENGINE API) are treated as **explicit non-goals** — `rustls` has no
  ENGINE-equivalent, and upstream Node itself is deprecating/removing this
  surface. RTS should document the option as accepted-but-ignored-with-warning
  or outright rejected, not silently misbehave.
- **Legacy `secureProtocol` method strings** (e.g. `'SSLv3_method'`,
  `'TLSv1_method'`) that select protocols below TLS 1.2 are not supported by
  `rustls` at all — this is a deliberate, intentional deviation from Node
  parity (documented here, not silently discovered later); RTS should reject
  such values with a clear unsupported-protocol error rather than silently
  downgrading or ignoring the option.
- **`NODE_TLS_REJECT_UNAUTHORIZED` env var** compat is unresolved — whether
  RTS supports this global override at all, and if so, whether it emits the
  same runtime warning Node does, needs an owner decision (§5.7).
- **HTTP/2 (`node:http2`)** ALPN negotiating to `'h2'` and then actually
  speaking HTTP/2 framing is entirely out of scope for `node:https`; this
  module only negotiates ALPN and always speaks HTTP/1.1 on the wire, same
  as real Node's `node:https` (h2 is a separate module built on the same TLS
  primitives).
- **Proxy tunneling for HTTPS targets** (`CONNECT`-based tunneling through
  an `http.Agent`-configured HTTP(S) proxy, `proxyEnv`) is deferred to the
  same follow-up noted in the `node:http` spec's §7 — full tunneling
  semantics through a proxy for `https.request` are documented only in
  outline here.
- **`node:tls` as its own module** (raw `tls.connect`/`tls.createServer`,
  `tls.checkServerIdentity`, `tls.rootCertificates`, `tls.getCiphers()`, etc.)
  is a separate spec; this document only covers the `https`-exported surface
  and the minimum `tls.*` types/behaviors it references. Building `node:https`
  first and extracting a shared TLS core into `node:tls` afterward (or vice
  versa) is an implementation-order choice left open here.
