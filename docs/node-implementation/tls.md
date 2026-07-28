# node:tls

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:tls` |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P1 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import tls from 'node:tls'`; `import { connect, createServer, createSecureContext, checkServerIdentity, TLSSocket, Server, SecureContext } from 'node:tls'`; `const tls = require('node:tls')` |
| Globals exposed | none (all access is via the `node:tls` module import; no ambient globals) |

## 1. Purpose

`node:tls` implements TLS (and legacy SSL) encrypted stream sockets on top of `node:net`. It provides a client connector (`tls.connect`), a server (`tls.createServer`/`tls.Server`), the encrypted-duplex-stream wrapper (`tls.TLSSocket`) that can wrap either a fresh OS socket or an already-connected plain `net.Socket`, and the `tls.SecureContext` object that packages certificate/key/cipher/protocol-version configuration into a reusable, immutable TLS configuration. It is the shared low-level TLS core that `node:https`, `node:http2`, and any other TLS-over-TCP module builds on top of — `node:https`'s own spec (`docs/node-implementation/https.md` §7) explicitly defers the TLS core to this document. Everything security-relevant in the module (cipher selection, certificate verification, session resumption, renegotiation, SNI/ALPN/PSK negotiation, OCSP stapling) is OpenSSL-shaped API surface that RTS re-implements over `rustls` rather than OpenSSL/BoringSSL — see §5.1 and §7 for where that substitution changes observable behavior.

## 2. Exported API surface (COMPLETE)

### Classes

#### `tls.Server`

Extends: `net.Server` (→ `EventEmitter`). Created via `tls.createServer([options][, secureConnectionListener])` — there is no public `new tls.Server(...)` constructor documented as primary API (the class exists so `instanceof tls.Server` and subclass-based typing work; `createServer` is the only supported construction path).

```typescript
class Server extends net.Server {
  addContext(hostname: string, context: SecureContextOptions | tls.SecureContext): void;
  address(): AddressInfo | string | null;                 // inherited from net.Server, unchanged shape
  close(callback?: (err?: Error) => void): this;
  getTicketKeys(): Buffer;                                 // 48 bytes
  listen(...args: unknown[]): this;                          // identical overload set to net.Server.listen()
  setSecureContext(options: SecureContextOptions): void;
  setTicketKeys(keys: Buffer | NodeJS.TypedArray | DataView): void; // 48 bytes

  readonly maxConnections: number;                          // inherited from net.Server
}
```

Instance methods:

| Method | Signature | Return type |
|---|---|---|
| `addContext` | `addContext(hostname: string, context: SecureContextOptions \| tls.SecureContext): void` | `void` |
| `address` | `address(): AddressInfo \| string \| null` | inherited from `net.Server` |
| `close` | `close(callback?: (err?: Error) => void): this` | `tls.Server` |
| `getTicketKeys` | `getTicketKeys(): Buffer` | `Buffer` (48 bytes) |
| `listen` | `listen(...)` | inherited unchanged from `net.Server.listen()` |
| `setSecureContext` | `setSecureContext(options: SecureContextOptions): void` | `void` |
| `setTicketKeys` | `setTicketKeys(keys: Buffer \| NodeJS.TypedArray \| DataView): void` | `void` |

Instance properties: `maxConnections` (inherited from `net.Server`; `Infinity` if unset). No TLS-specific own properties.

Events:

| Event | Handler signature | Notes |
|---|---|---|
| `'connection'` | `(socket: stream.Duplex) => void` | inherited from `net.Server`; fires with the **raw, not-yet-handshaked** socket, before TLS negotiation completes |
| `'keylog'` | `(line: Buffer, tlsSocket: tls.TLSSocket) => void` | one line per key-material event, NSS `SSLKEYLOGFILE` format; only emitted while a listener is attached |
| `'newSession'` | `(sessionId: Buffer, sessionData: Buffer, callback: () => void) => void` | custom session-ID cache store hook (TLSv1.2 and below); handshake blocks until `callback()` is invoked |
| `'OCSPRequest'` | `(certificate: Buffer, issuer: Buffer, callback: (err: Error \| null, response?: Buffer) => void) => void` | requires `requestCert`-independent OCSP stapling support; `certificate`/`issuer` are DER `Buffer`s |
| `'resumeSession'` | `(sessionId: Buffer, callback: (err: Error \| null, sessionData: Buffer \| null) => void) => void` | custom session-ID cache lookup hook (TLSv1.2 and below) |
| `'secureConnection'` | `(tlsSocket: tls.TLSSocket) => void` | fires once the TLS handshake completes successfully |
| `'tlsClientError'` | `(exception: Error, tlsSocket: tls.TLSSocket) => void` | handshake-level failure (before `'secureConnection'` would have fired); socket is destroyed after this fires |

#### `tls.TLSSocket`

Extends: `net.Socket` (→ `stream.Duplex` → `EventEmitter`).

```typescript
class TLSSocket extends net.Socket {
  constructor(socket: net.Socket | stream.Duplex, options?: TLSSocketOptions);

  address(): AddressInfo | {};                                            // inherited shape from net.Socket
  disableRenegotiation(): void;
  enableTrace(): void;
  exportKeyingMaterial(length: number, label: string, context?: Buffer): Buffer;
  getCertificate(): PeerCertificate | object | null;                      // own (local) certificate
  getCipher(): CipherNameAndProtocol | undefined;
  getEphemeralKeyInfo(): EphemeralKeyInfo | object | null;
  getFinished(): Buffer | undefined;
  getPeerCertificate(detailed?: boolean): PeerCertificate | DetailedPeerCertificate | object;
  getPeerFinished(): Buffer | undefined;
  getPeerX509Certificate(): X509Certificate | undefined;                  // node:crypto's X509Certificate
  getProtocol(): string | null;
  getSession(): Buffer | undefined;
  getSharedSigalgs(): string[];
  getTLSTicket(): Buffer | undefined;
  getX509Certificate(): X509Certificate | undefined;                      // node:crypto's X509Certificate
  isSessionReused(): boolean;
  renegotiate(options: SecureContextOptions & { rejectUnauthorized?: boolean; requestCert?: boolean }, callback?: (err: Error | null) => void): boolean;
  setKeyCert(context: SecureContextOptions | tls.SecureContext): void;
  setMaxSendFragment(size: number): boolean;                              // max 16384

  readonly authorizationError: Error | null;
  readonly authorized: boolean;
  readonly encrypted: true;
  readonly localAddress: string | undefined;
  readonly localPort: number | undefined;
  readonly remoteAddress: string | undefined;
  readonly remoteFamily: string | undefined;
  readonly remotePort: number | undefined;
  alpnProtocol: string | false | null;                                    // negotiated ALPN protocol, or false/null if none
  servername: string | undefined;                                         // SNI hostname the client sent (server-side)
}
```

Instance methods:

| Method | Signature | Return type |
|---|---|---|
| `address` | `address(): AddressInfo \| {}` | inherited from `net.Socket` |
| `disableRenegotiation` | `disableRenegotiation(): void` | `void` |
| `enableTrace` | `enableTrace(): void` | `void` (writes packet trace to stderr) |
| `exportKeyingMaterial` | `exportKeyingMaterial(length: number, label: string, context?: Buffer): Buffer` | `Buffer` |
| `getCertificate` | `getCertificate(): PeerCertificate \| object \| null` | own local cert, or `{}` if none, or `null` before handshake |
| `getCipher` | `getCipher(): CipherNameAndProtocol \| undefined` | `{name, standardName, version}` |
| `getEphemeralKeyInfo` | `getEphemeralKeyInfo(): EphemeralKeyInfo \| object \| null` | `null` if not a client socket; `{}` for non-ephemeral kx (e.g. PSK) |
| `getFinished` | `getFinished(): Buffer \| undefined` | latest `Finished` message sent |
| `getPeerCertificate` | `getPeerCertificate(detailed?: boolean): PeerCertificate \| DetailedPeerCertificate \| object` | `{}` if no peer cert; `detailed=true` includes `issuerCertificate` chain |
| `getPeerFinished` | `getPeerFinished(): Buffer \| undefined` | latest `Finished` message expected from peer |
| `getPeerX509Certificate` | `getPeerX509Certificate(): X509Certificate \| undefined` | since v15.9.0 |
| `getProtocol` | `getProtocol(): string \| null` | `'TLSv1.3'` / `'TLSv1.2'` / `'unknown'` / `null` (not yet connected) |
| `getSession` | `getSession(): Buffer \| undefined` | TLSv1.2 and below only |
| `getSharedSigalgs` | `getSharedSigalgs(): string[]` | signature algorithms both peers advertised |
| `getTLSTicket` | `getTLSTicket(): Buffer \| undefined` | session ticket, client-side |
| `getX509Certificate` | `getX509Certificate(): X509Certificate \| undefined` | own local cert as `X509Certificate` |
| `isSessionReused` | `isSessionReused(): boolean` | `true` if the handshake resumed a session |
| `renegotiate` | `renegotiate(options, callback?): boolean` | `false` if renegotiation cannot be initiated (see §4/§7) |
| `setKeyCert` | `setKeyCert(context): void` | since v22.5.0; used from an `ALPNCallback` to switch key/cert per negotiated protocol |
| `setMaxSendFragment` | `setMaxSendFragment(size: number): boolean` | `false` if `size` out of range (`[512, 16384]`) |

Instance properties:

| Property | Type | Notes |
|---|---|---|
| `authorizationError` | `Error \| null` | set only when `authorized === false` |
| `authorized` | `boolean` | `true` if the peer certificate was verified against the supplied/default CA list |
| `encrypted` | `true` | always `true`; distinguishes a `TLSSocket` from a plain `net.Socket` duck-type-wise |
| `localAddress` | `string \| undefined` | string form of local IP |
| `localPort` | `number \| undefined` | numeric local port |
| `remoteAddress` | `string \| undefined` | string form of remote IP |
| `remoteFamily` | `string \| undefined` | `'IPv4'` or `'IPv6'` |
| `remotePort` | `number \| undefined` | numeric remote port |
| `alpnProtocol` | `string \| false \| null` | negotiated ALPN protocol name; `false` if ALPN was attempted but no protocol matched |
| `servername` | `string \| undefined` | server-side: the SNI hostname the client sent |

Events:

| Event | Handler signature | Notes |
|---|---|---|
| `'keylog'` | `(line: Buffer) => void` | per-socket key-material line, NSS `SSLKEYLOGFILE` format |
| `'OCSPResponse'` | `(response: Buffer) => void` | client-side, only when `requestOCSP: true` was set |
| `'secure'` | `() => void` | handshake complete; fires for both client- and server-created `TLSSocket`s (including ones built via `new tls.TLSSocket()`) |
| `'secureConnect'` | `() => void` | handshake complete; **client-only**, and NOT emitted for a `TLSSocket` constructed directly via `new tls.TLSSocket(socket, options)` (only for sockets from `tls.connect()`) |
| `'session'` | `(session: Buffer) => void` | client-side; new session ticket/data available for resumption; may fire multiple times under TLSv1.3 |

Plus every event inherited from `net.Socket`/`stream.Duplex` (`'close'`, `'connect'`, `'data'`, `'drain'`, `'end'`, `'error'`, `'lookup'`, `'ready'`, `'timeout'`) — unchanged semantics, documented in the `node:net` spec.

#### `tls.SecureContext`

Not directly instantiated by user code with `new`; the only supported construction path is `tls.createSecureContext([options])`. Opaque wrapper around the underlying TLS configuration (an OpenSSL `SSL_CTX` in real Node; an immutable `rustls::{ClientConfig, ServerConfig}` pair in RTS — see §5.1). No public methods, instance properties, or events are documented on this class; it is consumed by `secureContext` options on `tls.connect`/`tls.createServer`/`tlsSocket.setKeyCert`/`server.addContext`/`server.setSecureContext`.

### Top-level functions

| Function | Variant |
|---|---|
| `tls.checkServerIdentity(hostname, cert)` | sync |
| `tls.connect(options[, callback])` | callback (via event, not Node-style error-first) |
| `tls.connect(path[, options][, callback])` | callback (via event) |
| `tls.connect(port[, host][, options][, callback])` | callback (via event) |
| `tls.createSecureContext([options])` | sync |
| `tls.createServer([options][, secureConnectionListener])` | sync (constructs; I/O is async) |
| `tls.getCACertificates([type])` | sync |
| `tls.getCiphers()` | sync |
| `tls.setDefaultCACertificates(certs)` | sync |

#### `tls.checkServerIdentity(hostname, cert)`

The default hostname-verification algorithm RTS/Node run against the peer certificate's Subject Alternative Names (and, as a legacy fallback, Common Name) unless overridden by a user-supplied `checkServerIdentity` option.

| Name | Type | Optional | Default |
|---|---|---|---|
| `hostname` | `string` | no | — |
| `cert` | `PeerCertificate` (as returned by `getPeerCertificate()`) | no | — |

Return: `Error | undefined` (an `Error` describing the mismatch — `reason`, `host`, `cert` fields set on it — or `undefined` if the hostname matches). Throws: none (returns the `Error`, does not throw it — the caller, e.g. the handshake path, is what raises/rejects). Variant: sync. Note: does **not** check `uniformResourceIdentifier` subject-alt-name entries — removed as a security fix for CVE-2021-44531 and never to be reintroduced (§4).

#### `tls.connect(options[, callback])` / `tls.connect(path[, options][, callback])` / `tls.connect(port[, host][, options][, callback])`

Opens a new TLS connection (or wraps an existing `socket`) and returns a `tls.TLSSocket` synchronously; the handshake itself completes asynchronously and is signaled via the `'secureConnect'` event (or `callback`, which is registered as a one-shot `'secureConnect'` listener — **not** an error-first Node callback).

| Name | Type | Optional | Default |
|---|---|---|---|
| `options` | `ConnectionOptions` | one of the three overloads must supply connection info | — |
| `path` | `string` | overload 2 only | — |
| `port` | `number` | overload 3 only | — |
| `host` | `string` | overload 3 only | `'localhost'` |
| `callback` | `() => void` | yes | registered as a one-time `'secureConnect'` listener |

Return: `tls.TLSSocket`. Throws: `TypeError`/`ERR_INVALID_ARG_TYPE` synchronously on malformed argument shapes; connection/handshake failures surface via the socket's `'error'` event, not a thrown exception. Variant: callback (event-based; the returned socket is usable immediately in a paused/buffering state before the handshake finishes).

#### `tls.createSecureContext([options])`

| Name | Type | Optional | Default |
|---|---|---|---|
| `options` | `SecureContextOptions` | yes | `{}` (uses RTS/Node's default cipher suite, bundled root CAs, no client cert) |

Return: `tls.SecureContext`. Throws: `Error` synchronously on malformed PEM/PKCS12 input, bad passphrase, or an invalid `ciphers`/`minVersion`/`maxVersion` combination (`ERR_TLS_INVALID_PROTOCOL_VERSION`, `ERR_TLS_PROTOCOL_VERSION_CONFLICT`, `ERR_TLS_INVALID_CONTEXT`). Variant: sync.

#### `tls.createServer([options][, secureConnectionListener])`

| Name | Type | Optional | Default |
|---|---|---|---|
| `options` | `TlsOptions` | yes | `{}` |
| `secureConnectionListener` | `(socket: tls.TLSSocket) => void` | yes | registered as a one-time-per-connection `'secureConnection'` listener |

Return: `tls.Server`. Throws: same synchronous context-construction errors as `createSecureContext` if `options` embeds bad cert/key material; `key`/`cert` (or `pfx`) are effectively required for a server that will actually accept connections (a context-less server can be constructed but every incoming handshake fails). Variant: sync (construction); the server itself does async I/O once `.listen()` is called.

#### `tls.getCACertificates([type])`

| Name | Type | Optional | Default |
|---|---|---|---|
| `type` | `'default' \| 'system' \| 'bundled' \| 'extra'` | yes | `'default'` |

Return: `string[]` — PEM-encoded certificates for the requested store. `'default'`: whatever `tls.rootCertificates` + `NODE_EXTRA_CA_CERTS` currently resolve to; `'bundled'`: the Mozilla-derived bundle RTS/Node ship; `'system'`: OS trust store (only meaningful with `--use-system-ca`); `'extra'`: certs loaded from `NODE_EXTRA_CA_CERTS`. Throws: `TypeError` on an unrecognized `type` string. Variant: sync. (Added v23.6.0 upstream — new since the historical `tls.rootCertificates`-only surface.)

#### `tls.getCiphers()`

No params. Return: `string[]` — lower-case cipher-suite names RTS's TLS backend supports (see §5.1 for how this differs from OpenSSL's ~300-entry universe). Variant: sync.

#### `tls.setDefaultCACertificates(certs)`

| Name | Type | Optional | Default |
|---|---|---|---|
| `certs` | `string[]` (PEM-encoded certs) | no | — |

Return: `void`. Replaces the process-wide default CA list consulted whenever a `SecureContext` is created without an explicit `ca` option. Throws: `TypeError` on malformed PEM. Variant: sync. (Added v23.6.0/v22.11.0 upstream — pairs with `getCACertificates`.)

### Properties & constants

| Property | Type | Mutability | Notes |
|---|---|---|---|
| `tls.rootCertificates` | `readonly string[]` | frozen array | Array of PEM-encoded root CA certificates RTS/Node bundle (Mozilla-derived set) |
| `tls.DEFAULT_ECDH_CURVE` | `string` | **mutable** (module-level `let`) | Default ECDH curve name used when `ecdhCurve` option is unset; default `'auto'` |
| `tls.DEFAULT_MAX_VERSION` | `string` | **mutable** | Default max TLS protocol version when `maxVersion` unset; `'TLSv1.3'` |
| `tls.DEFAULT_MIN_VERSION` | `string` | **mutable** | Default min TLS protocol version when `minVersion` unset; `'TLSv1.2'` |
| `tls.DEFAULT_CIPHERS` | `string` | **mutable** | Default OpenSSL-cipher-list-format string consulted when `ciphers` unset; users commonly do `tls.DEFAULT_CIPHERS += ':!WEAK'` at process startup |
| `tls.CLIENT_RENEG_LIMIT` | `number` | **mutable** | Max renegotiations tolerated per `CLIENT_RENEG_WINDOW`; default `3` |
| `tls.CLIENT_RENEG_WINDOW` | `number` | **mutable** | Renegotiation counting window, in seconds; default `600` |

All six `DEFAULT_*`/`CLIENT_RENEG_*` entries are genuinely mutable module bindings that later `createSecureContext`/`connect`/`createServer` calls must re-read at call time (not baked in once at module load) — see §5.2/§5.6.

### Events

Module-level: none (events live on `tls.Server` and `tls.TLSSocket` instances, listed above).

## 3. Types & option objects

```typescript
type BufferEncoding =
  | "ascii" | "utf8" | "utf-8" | "utf16le" | "utf-16le" | "ucs2" | "ucs-2"
  | "base64" | "base64url" | "latin1" | "binary" | "hex";

// -- canonical definitions; node:https restates these for its own doc's completeness --

interface SecureContextOptions {
  ca?: string | string[] | Buffer | Buffer[];              // trusted CA cert(s); default: tls.rootCertificates (+ NODE_EXTRA_CA_CERTS)
  cert?: string | string[] | Buffer | Buffer[];             // cert chain, PEM
  ciphers?: string;                                         // OpenSSL cipher-list format; default: tls.DEFAULT_CIPHERS
  clientCertEngine?: string;                                 // deprecated, OpenSSL ENGINE API removed upstream — see §4
  crl?: string | string[] | Buffer | Buffer[];               // PEM CRL(s)
  dhparam?: string | Buffer;                                 // DHE params; 'auto' requests well-known groups (see §7 — unsupported under rustls)
  ecdhCurve?: string;                                        // default tls.DEFAULT_ECDH_CURVE ('auto')
  honorCipherOrder?: boolean;
  key?: string | string[] | Buffer | Buffer[] | KeyObject | Array<{ pem: string | Buffer; passphrase?: string }>;
  passphrase?: string;                                       // decrypts key/pfx
  pfx?: string | Buffer | Array<{ buf: string | Buffer; passphrase?: string }>;
  privateKeyEngine?: string;                                  // deprecated — see §4
  privateKeyIdentifier?: string;                              // deprecated — see §4
  secureOptions?: number;                                     // OpenSSL SSL_OP_* bitmask (best-effort under rustls — see §7)
  secureProtocol?: string;                                    // legacy method name e.g. 'TLSv1_2_method'; prefer minVersion/maxVersion
  sessionIdContext?: string;
  sigalgs?: string;                                           // colon-separated signature algorithm list
  sessionTimeout?: number;                                    // session cache lifetime, seconds; default 300
  ticketKeys?: Buffer;                                        // 48 bytes
  minVersion?: "TLSv1.3" | "TLSv1.2" | "TLSv1.1" | "TLSv1";   // default tls.DEFAULT_MIN_VERSION
  maxVersion?: "TLSv1.3" | "TLSv1.2" | "TLSv1.1" | "TLSv1";   // default tls.DEFAULT_MAX_VERSION
}

interface TlsOptions extends SecureContextOptions {
  // net.ServerOptions fields (see node:net spec):
  allowHalfOpen?: boolean;          // default false
  pauseOnConnect?: boolean;         // default false
  noDelay?: boolean;
  keepAlive?: boolean;
  keepAliveInitialDelay?: number;

  // TLS-specific server options:
  ALPNProtocols?: string[] | Buffer | NodeJS.TypedArray | DataView;
  ALPNCallback?: (arg: { servername: string | null; protocols: string[] }) => string | undefined;
  SNICallback?: (servername: string, callback: (err: Error | null, ctx?: tls.SecureContext) => void) => void;
  clientCertEngine?: string;                    // deprecated — see §4
  enableTrace?: boolean;
  handshakeTimeout?: number;                    // default 120000 (ms)
  rejectUnauthorized?: boolean;                  // default: true when requestCert is true
  requestCert?: boolean;                         // default false
  sessionTimeout?: number;
  pskCallback?: (socket: tls.TLSSocket, identity: string) => Buffer | null;
  pskIdentityHint?: string;
  secureContext?: tls.SecureContext;             // pre-built; if unset, one is built from the SecureContextOptions fields above
  ticketKeys?: Buffer;
}

interface ConnectionOptions extends SecureContextOptions {
  host?: string;                                  // default 'localhost'
  port?: number;
  path?: string;                                  // Unix socket path; overrides host/port
  socket?: stream.Duplex;                         // existing connected socket to wrap; overrides host/port/path
  allowHalfOpen?: boolean;                        // default false
  rejectUnauthorized?: boolean;                    // default true
  pskCallback?: (hint: string | null) => { psk: Buffer; identity: string } | null;
  ALPNProtocols?: string[] | Buffer | NodeJS.TypedArray | DataView;
  servername?: string;                             // SNI hostname
  checkServerIdentity?: (servername: string, cert: PeerCertificate) => Error | undefined;
  session?: Buffer;                                // resumption blob from a prior 'session' event
  requestOCSP?: boolean;
  minDHSize?: number;                              // default 1024
  highWaterMark?: number;                          // default 16 * 1024
  secureContext?: tls.SecureContext;
  onread?: { buffer: Buffer | (() => Buffer); callback: (bytesWritten: number, buf: Buffer) => boolean };
  lookup?: (hostname: string, options: object, cb: (err: NodeJS.ErrnoException | null, address: string, family: number) => void) => void;
  timeout?: number;
}

interface TLSSocketOptions extends SecureContextOptions {
  enableTrace?: boolean;
  isServer?: boolean;                              // default false
  server?: net.Server;
  requestCert?: boolean;
  rejectUnauthorized?: boolean;
  ALPNProtocols?: string[] | Buffer | NodeJS.TypedArray | DataView;
  ALPNCallback?: (arg: { servername: string | null; protocols: string[] }) => string | undefined;
  SNICallback?: (servername: string, callback: (err: Error | null, ctx?: tls.SecureContext) => void) => void;
  session?: Buffer;
  requestOCSP?: boolean;
  secureContext?: tls.SecureContext;
  pskCallback?: Function;
  pskIdentityHint?: string;
}

interface PeerCertificate {
  subject: Record<string, string>;
  issuer: Record<string, string>;
  subjectaltname?: string;                          // e.g. 'DNS:*.example.com, DNS:example.com'
  infoAccess?: Record<string, string[]>;
  valid_from: string;
  valid_to: string;
  serialNumber: string;
  fingerprint: string;                              // SHA-1, colon-separated hex
  fingerprint256: string;                           // SHA-256
  fingerprint512: string;                           // SHA-512 (since v17.2.0)
  ext_key_usage?: string[];
  ca?: boolean;
  raw: Buffer;                                       // DER-encoded certificate
  pubkey?: Buffer;
  bits?: number;                                      // RSA: key size in bits
  exponent?: string;                                  // RSA: e.g. '0x10001'
  modulus?: string;                                   // RSA: hex modulus
  asn1Curve?: string;                                  // EC: e.g. 'prime256v1'
  nistCurve?: string;                                  // EC: e.g. 'P-256'
}

interface DetailedPeerCertificate extends PeerCertificate {
  issuerCertificate?: DetailedPeerCertificate;         // recursive chain, undefined at the root
}

interface CipherNameAndProtocol {
  name: string;           // e.g. 'TLS_AES_256_GCM_SHA384'
  standardName: string;   // RFC cipher-suite name
  version: string;        // protocol version the cipher was negotiated under, e.g. 'TLSv1.3'
}

interface EphemeralKeyInfo {
  type: 'DH' | 'ECDH';
  name?: string;           // curve name for ECDH
  size: number;            // bits
}

type NewSessionCallback = () => void;
type ResumeSessionCallback = (err: Error | null, sessionData: Buffer | null) => void;
type OCSPRequestCallback = (err: Error | null, response?: Buffer) => void;
type SNICallback = (servername: string, callback: (err: Error | null, ctx?: tls.SecureContext) => void) => void;
type ALPNCallback = (arg: { servername: string | null; protocols: string[] }) => string | undefined;
type PSKServerCallback = (socket: tls.TLSSocket, identity: string) => Buffer | null;
type PSKClientCallback = (hint: string | null) => { psk: Buffer; identity: string } | null;
type CheckServerIdentityCallback = (servername: string, cert: PeerCertificate) => Error | undefined;
```

Note on `X509Certificate` and `KeyObject`: both are `node:crypto` classes (see `docs/node-implementation/crypto.md` — `X509Certificate` §"Classes", `KeyObject` §"Classes") reused here as return/param types for `getPeerX509Certificate()`, `getX509Certificate()`, and `SecureContextOptions.key`; `node:tls` does not redefine them.

## 4. Node semantics & edge cases

- **Protocol version range.** Default `minVersion` is `'TLSv1.2'`, default `maxVersion` is `'TLSv1.3'`. `'SSLv3'`, `'TLSv1'`, `'TLSv1.1'` remain accepted as **explicit** `minVersion`/`maxVersion`/`secureProtocol` values for legacy interop in real Node (subject to the underlying OpenSSL build actually supporting them), but are never negotiated by default. `secureProtocol` (legacy OpenSSL method-name strings like `'TLSv1_2_method'`) is superseded by `minVersion`/`maxVersion` and should be treated as legacy input only.
- **Perfect forward secrecy.** ECDHE is enabled by default for all supported cipher suites; DHE is opt-in via `dhparam: 'auto'` (or explicit PEM params) and disabled by default. Under TLSv1.3, an ephemeral (EC)DHE exchange is used for every non-PSK-only handshake — PFS is not optional there.
- **Cipher suite / OpenSSL security level.** `ciphers` follows OpenSSL cipher-list string syntax (colon-separated, `!`/`-`/`+` modifiers, `@SECLEVEL=N` suffix). `tls.DEFAULT_CIPHERS` is process-global and mutable — code that tightens it (`tls.DEFAULT_CIPHERS += ':!RC4'`) must do so **before** the first `createServer`/`connect`/`createSecureContext` call for the change to take effect everywhere that doesn't pass an explicit `ciphers` option.
- **SNI (Server Name Indication).** Clients should always set `servername` (defaults to the `host` option in `tls.connect`, but not when connecting via `path`/an existing `socket`). Servers select a per-hostname `SecureContext` via `SNICallback` or the `addContext(hostname, context)` map; if neither is set, the server's single default context answers every hostname.
- **ALPN.** `ALPNProtocols` (client and server) or a server-side `ALPNCallback` picks the negotiated application protocol; `socket.alpnProtocol` reflects the result (`string` on match, `false` if ALPN was attempted with no match, `null`/`undefined` if ALPN wasn't attempted at all). `setKeyCert()` (v22.5.0+) lets a server switch key/cert material from inside `ALPNCallback`, after SNI has already selected a context but before the certificate is sent.
- **PSK (pre-shared key).** Off by default; requires explicitly listing a PSK cipher suite in `ciphers` (e.g. `'PSK-AES128-GCM-SHA256'`) plus a `pskCallback`. PSK identities are capped at 128 bytes and PSK values at 256 bytes under OpenSSL 1.1.0. Because there is no certificate exchanged, `checkServerIdentity` must be overridden (typically to a no-op) or the handshake's default hostname check will reject the connection.
- **Session resumption — two mechanisms.** (1) **Session-ID based** (TLSv1.2 and below): server emits `'newSession'`/`'resumeSession'` to let user code externalize the session cache (e.g. to Redis for a server farm); (2) **Ticket based** (all versions, the default on the server side once `ticketKeys`/`sessionTimeout` are configured): the server encrypts session state into an opaque ticket the client replays; no server-side cache needed, but **every server instance in a farm must share the same `ticketKeys`** for a client to resume against a different instance than the one it first connected to. Under TLSv1.3, the server may send **multiple** tickets after the handshake, so a client's `'session'` event can fire more than once per connection; `getSession()` only returns usable data for TLSv1.2 and below — TLSv1.3 resumption data must be captured via the `'session'` event.
- **Client-initiated renegotiation mitigation.** `tls.CLIENT_RENEG_LIMIT` (default 3) / `tls.CLIENT_RENEG_WINDOW` (default 600s) bound how many renegotiations a peer may request before the connection is torn down with an `Error` whose `code` is `'ERR_TLS_RENEGOTIATION_LIMIT'`, mitigating a DoS vector where renegotiation is disproportionately expensive for the server relative to the client. TLSv1.3 has no renegotiation at all (removed from the protocol), so these constants and `disableRenegotiation()`/`renegotiate()` are meaningful only for TLSv1.2-and-below connections.
- **OCSP stapling.** Client sets `requestOCSP: true` and listens for `'OCSPResponse'`. Server listens for `'OCSPRequest'` — receives the DER `certificate`/`issuer` buffers, is expected to perform (or have cached) the actual OCSP lookup against the issuing CA, and calls back with the raw OCSP response bytes (or `null`/an error).
- **X.509 verification error taxonomy.** When `authorized === false`, `authorizationError`/the handshake failure error's `.code` is one of the OpenSSL X.509 verify result names: `UNABLE_TO_GET_ISSUER_CERT`, `UNABLE_TO_GET_CRL`, `UNABLE_TO_DECRYPT_CERT_SIGNATURE`, `UNABLE_TO_DECRYPT_CRL_SIGNATURE`, `UNABLE_TO_DECODE_ISSUER_PUBLIC_KEY`, `CERT_SIGNATURE_FAILURE`, `CRL_SIGNATURE_FAILURE`, `CERT_NOT_YET_VALID`, `CERT_HAS_EXPIRED`, `CRL_NOT_YET_VALID`, `CRL_HAS_EXPIRED`, `ERROR_IN_CERT_NOT_BEFORE_FIELD`, `ERROR_IN_CERT_NOT_AFTER_FIELD`, `ERROR_IN_CRL_LAST_UPDATE_FIELD`, `ERROR_IN_CRL_NEXT_UPDATE_FIELD`, `OUT_OF_MEM`, `DEPTH_ZERO_SELF_SIGNED_CERT`, `SELF_SIGNED_CERT_IN_CHAIN`, `UNABLE_TO_GET_ISSUER_CERT_LOCALLY`, `UNABLE_TO_VERIFY_LEAF_SIGNATURE`, `CERT_CHAIN_TOO_LONG`, `CERT_REVOKED`, `INVALID_CA`, `PATH_LENGTH_EXCEEDED`, `INVALID_PURPOSE`, `CERT_UNTRUSTED`, `CERT_REJECTED`, `HOSTNAME_MISMATCH`. These are distinct from the Node-level `ERR_TLS_*` codes below.
- **Node-level `ERR_TLS_*` error codes** (thrown/emitted as `Error.code`): `ERR_TLS_CERT_ALTNAME_FORMAT`, `ERR_TLS_CERT_ALTNAME_INVALID`, `ERR_TLS_DH_PARAM_SIZE`, `ERR_TLS_HANDSHAKE_TIMEOUT`, `ERR_TLS_INVALID_CONTEXT`, `ERR_TLS_INVALID_PROTOCOL_METHOD`, `ERR_TLS_INVALID_PROTOCOL_VERSION`, `ERR_TLS_INVALID_STATE`, `ERR_TLS_PROTOCOL_VERSION_CONFLICT`, `ERR_TLS_RENEGOTIATION_DISABLED`, `ERR_TLS_RENEGOTIATION_FAILED`, `ERR_TLS_REQUIRED_SERVER_NAME`, `ERR_TLS_SESSION_ATTACK`, `ERR_TLS_SNI_FROM_SERVER` (verify exact current membership against Node 25's `lib/internal/errors.js` at implementation time — this list is stable across recent majors but individual entries have been added/removed historically). Underlying OpenSSL failures also surface as dynamically-named `ERR_SSL_*` codes (e.g. `ERR_SSL_WRONG_VERSION_NUMBER`) derived directly from the OpenSSL error queue — RTS's `rustls` backend does not produce these verbatim (see §7).
- **Deprecations / removed APIs.** `tls.createSecurePair()` and the legacy `tls.SecurePair`/`tls.CryptoStream` classes were removed in Node 17 (verify exact version) — never implement them. `checkServerIdentity()` no longer honors `uniformResourceIdentifier` Subject Alternative Name entries as of the fix for **CVE-2021-44531** (Node v17.3.1/v16.13.2/v14.18.3/v12.22.9) — a security fix, not a feature to restore. `clientCertEngine`/`privateKeyEngine`/`privateKeyIdentifier` are deprecated following OpenSSL's own ENGINE API removal upstream; RTS's `rustls` backend has no engine concept at all (see §7). NPN (Next Protocol Negotiation, ALPN's predecessor) was removed from Node entirely years ago and must not be implemented.
- **Platform notes.** On POSIX, the default trust anchor is Node's bundled Mozilla-derived root store (`tls.rootCertificates`), independent of the OS store, unless `--use-system-ca`/`--use-bundled-ca` CLI flags or `tls.setDefaultCACertificates()`/an explicit `ca` option change that. On Windows, the same bundled-by-default behavior applies; `--use-system-ca` pulls from the Windows certificate store (`CryptoAPI`/`CertOpenStore`) instead. `tls.getCACertificates('system')` surfaces whichever OS store integration is active.
- **Ordering guarantees.** `'secureConnect'`/`'secureConnection'` always fires strictly after the handshake completes and strictly before any `'data'`/user-visible read on that socket; for `tls.Server`, the raw `'connection'` event fires **before** the handshake even begins (it hands out the not-yet-secure socket), which is a different ordering than `https.Server`'s own `'connection'` (fires after the handshake — see `https.md` §4).
- **Backpressure.** `TLSSocket` is a `Duplex` stream; `write()`/`'drain'`/`highWaterMark` semantics are identical to `net.Socket` — encryption/decryption does not introduce additional application-visible backpressure beyond normal stream flow control.
- **Security notes.** Never set `rejectUnauthorized: false` in production client code (defeats certificate verification entirely). PSK ciphers are a niche mutual-auth mechanism, not a general substitute for PKI. `'keylog'` output is extremely security-sensitive (it defeats TLS's confidentiality guarantee entirely for anyone who obtains the log) and must never be enabled by default or logged persistently in a production deployment.

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node`'s `node:tls` is a fully independent module: it owns its own TLS implementation and does **not** call into `rts-std`'s existing `tls` namespace (which the architecture doc lists as backing RTS's own `"rts:tls"` surface) — the two are deliberately parallel, not shared, per the rts-node independence decision.

| Area | Rust backing |
|---|---|
| TLS handshake, record layer (client + server) | `rustls`, wrapped for async I/O via `tokio-rustls` (`TlsConnector`/`TlsAcceptor`) — **`rts-node` takes its own direct Cargo dependency** on `rustls`/`tokio-rustls`, it does not reuse `rts-std`'s existing copy (see §5.7) |
| Root CA bundle | `webpki-roots` (embedded Mozilla set) for the bundled/default store; optional `rustls-native-certs` for `--use-system-ca`/`'system'` behavior (Windows `CryptoAPI` store, POSIX OpenSSL-style trust paths) |
| `SecureContext` construction (`ca`/`cert`/`key`/`pfx`/`ciphers`/version bounds) | a Rust builder over `rustls::{ClientConfig, ServerConfig}`; PEM parsing via `rustls-pemfile`, PKCS12 (`pfx`) via a `p12`/`pkcs12` crate, private-key normalization (PKCS1/PKCS8/SEC1) via `rustls-pemfile` + `pkcs8`/`sec1` crates |
| Certificate inspection (`PeerCertificate`/`DetailedPeerCertificate`, `getCertificate`/`getPeerCertificate`) | `x509-parser` (same crate `node:crypto`'s `X509Certificate` uses — see `crypto.md` §5.1) reading the DER bytes `rustls` hands back from the handshake |
| `getPeerX509Certificate`/`getX509Certificate` | delegate to `node:crypto`'s `X509Certificate` construction path (`__RTS_FN_NODE_CRYPTO_X509_NEW`) over the same DER bytes, returning a `crypto.X509Certificate` handle — cross-module reuse in-crate (both live under `rts-node`) |
| Session resumption | `rustls`'s built-in `ClientSessionStore` (client cache) and `rustls::server::{ProducesTickets, StoresServerSessions}` traits (server ticket/session-ID storage) drive the mechanics; the `'newSession'`/`'resumeSession'` user-overridable hooks are implemented as a custom `StoresServerSessions` impl that round-trips through the event queue (§5.3) |
| ALPN | native `rustls::{ClientConfig, ServerConfig}::alpn_protocols` field; `ALPNCallback` implemented via `rustls::server::ResolvesServerCert`-adjacent negotiation hook |
| SNI dispatch (`addContext`/`SNICallback`) | `rustls::server::ResolvesServerCert` trait implementation keyed by the SNI-hostname map built from `addContext` calls, or bridged to the user's `SNICallback` via the event queue |
| Ephemeral key info / shared sigalgs / Finished messages | read from `rustls`'s `ConnectionCommon`/handshake-state accessors where exposed; TLSv1.3's simplified handshake reduces how much of this is meaningfully populated versus TLSv1.2 (see §7) |
| `keylog` | `rustls::KeyLog` trait implementation feeding a per-handle line queue, same NSS `SSLKEYLOGFILE` text format Node produces |
| OCSP stapling | server-side: `rustls`'s `CertifiedKey.ocsp` field carries the stapled response bytes, populated by bridging the `'OCSPRequest'` event through the queue before the handshake can proceed; client-side: reading the stapled response off the connection and surfacing it as `'OCSPResponse'` |
| Renegotiation | **not implemented as a real operation** — `rustls` does not support in-band renegotiation at all (by design, considered legacy/insecure); `disableRenegotiation()`/`renegotiate()`/`CLIENT_RENEG_LIMIT`/`CLIENT_RENEG_WINDOW` are surface-compatible stubs (see §4/§7 for the explicit, justified regression) |
| PSK | best-effort/deferred — `rustls` has no first-class OpenSSL-style PSK-ciphersuite negotiation as of the versions considered; flagged as an open question (§7), not blocking the rest of the module |
| DHE / `dhparam` | **not supported** — `rustls` implements no finite-field Diffie-Hellman key exchange at all (ECDHE/X25519/hybrid PQ only); `dhparam` is accepted and silently ignored (or produces a clear "unsupported" error at `createSecureContext` time — implementation choice, see §7), an explicit justified regression vs OpenSSL |
| Byte-level TCP transport | reuses `node:net`'s Duplex socket implementation (raw accept-loop/connect/read/write) — `TLSSocket` wraps an existing `node:net` socket handle rather than reimplementing TCP itself, exactly like `node:https` layers over `node:http`'s connection machinery (`https.md` §5.1) |

### 5.2 ABI surface

Symbol convention: `__RTS_FN_NODE_TLS_<NAME>`. Rich/stateful objects (`SecureContext`, `TLSSocket`, `Server`) are opaque `Handle` (u64) values into `rts-node`'s own handle slab. Certificate/key/PSK/session material crosses as `StrPtr` (PEM, JSON) or `Handle` (a `Buffer`/`ArrayBuffer` handle for DER/binary blobs) — see §5.5. Compound structured results (`PeerCertificate`, `CipherNameAndProtocol`, `EphemeralKeyInfo`, event records) cross as a single JSON `StrPtr`, decoded by the `.ts` shim, mirroring the `node:dns`/`node:https` precedent of avoiding one bespoke ABI shape per small heterogeneous result type.

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_TLS_SECURE_CONTEXT_NEW` | `StrPtr optionsJson` | `Handle` | builds an immutable `rustls::{ClientConfig,ServerConfig}` pair from `SecureContextOptions`; backs both `tls.createSecureContext()` and the implicit context built inside `connect`/`createServer` when no `secureContext` option is given |
| `__RTS_FN_NODE_TLS_SECURE_CONTEXT_DESTROY` | `Handle` | `Void` | releases the handle-table slot; the underlying `Arc<rustls config>` may still be alive if referenced by live connections |
| `__RTS_FN_NODE_TLS_CONNECT` | `StrPtr optionsJson, Handle existingSocketOrZero, Handle secureContextOrZero` | `Handle` (TLSSocket) | if `existingSocketOrZero != 0`, wraps that `node:net` socket handle instead of opening a new TCP connection; handshake proceeds async, see §5.3 |
| `__RTS_FN_NODE_TLS_SOCKET_NEW` | `Handle plainSocket, Bool isServer, Handle secureContextOrZero, StrPtr optionsJson` | `Handle` (TLSSocket) | backs `new tls.TLSSocket(socket, options)`; does **not** auto-start the handshake read loop the same way `connect()`'s socket does (per `'secureConnect'` not firing for this constructor — §2) |
| `__RTS_FN_NODE_TLS_SOCKET_DESTROY` | `Handle` | `Void` | |
| `__RTS_FN_NODE_TLS_SOCKET_POLL_EVENT` | `Handle` | `Handle` (event record) or `0` if none pending | tag enum: `secure`/`secureConnect`/`session`/`keylog`/`OCSPResponse`/`error`/`close` (plus the inherited `net.Socket` data/end/drain/timeout tags — see `node:net` spec) |
| `__RTS_FN_NODE_TLS_SOCKET_GET_PEER_CERTIFICATE` | `Handle, Bool detailed` | `StrPtr` (JSON `PeerCertificate`/`DetailedPeerCertificate`, or `"{}"`) | |
| `__RTS_FN_NODE_TLS_SOCKET_GET_CERTIFICATE` | `Handle` | `StrPtr` (JSON, own cert) | |
| `__RTS_FN_NODE_TLS_SOCKET_GET_CIPHER` | `Handle` | `StrPtr` (JSON `CipherNameAndProtocol`) | |
| `__RTS_FN_NODE_TLS_SOCKET_GET_PROTOCOL` | `Handle` | `StrPtr` | `""`/null-sentinel before handshake completes |
| `__RTS_FN_NODE_TLS_SOCKET_GET_SESSION` | `Handle` | `Handle` (Buffer) or `0` | TLSv1.2 and below only |
| `__RTS_FN_NODE_TLS_SOCKET_GET_TLS_TICKET` | `Handle` | `Handle` (Buffer) or `0` | |
| `__RTS_FN_NODE_TLS_SOCKET_IS_SESSION_REUSED` | `Handle` | `Bool` | |
| `__RTS_FN_NODE_TLS_SOCKET_EXPORT_KEYING_MATERIAL` | `Handle, I32 length, StrPtr label, Handle contextBufOrZero` | `Handle` (Buffer) | RFC 5705 |
| `__RTS_FN_NODE_TLS_SOCKET_GET_EPHEMERAL_KEY_INFO` | `Handle` | `StrPtr` (JSON `EphemeralKeyInfo`, or `"{}"`/`null`) | |
| `__RTS_FN_NODE_TLS_SOCKET_GET_FINISHED` | `Handle` | `Handle` (Buffer) or `0` | |
| `__RTS_FN_NODE_TLS_SOCKET_GET_PEER_FINISHED` | `Handle` | `Handle` (Buffer) or `0` | |
| `__RTS_FN_NODE_TLS_SOCKET_GET_SHARED_SIGALGS` | `Handle` | `StrPtr` (JSON `string[]`) | |
| `__RTS_FN_NODE_TLS_SOCKET_SET_MAX_SEND_FRAGMENT` | `Handle, I32 size` | `Bool` | |
| `__RTS_FN_NODE_TLS_SOCKET_ENABLE_TRACE` | `Handle` | `Void` | |
| `__RTS_FN_NODE_TLS_SOCKET_DISABLE_RENEGOTIATION` | `Handle` | `Void` | no-op marker flag under `rustls` (§4/§7) |
| `__RTS_FN_NODE_TLS_SOCKET_RENEGOTIATE` | `Handle, StrPtr optionsJson` | `Bool` | always returns `false` under `rustls` (§7) |
| `__RTS_FN_NODE_TLS_SOCKET_SET_KEY_CERT` | `Handle, Handle secureContextOrZero, StrPtr keyCertJsonOrEmpty` | `Void` | |
| `__RTS_FN_NODE_TLS_SOCKET_GET_PEER_X509` | `Handle` | `Handle` (`crypto.X509Certificate`) or `0` | delegates to `node:crypto`'s X509 construction (§5.1) |
| `__RTS_FN_NODE_TLS_SOCKET_GET_X509` | `Handle` | `Handle` (`crypto.X509Certificate`) or `0` | |
| `__RTS_FN_NODE_TLS_SOCKET_AUTHORIZED` | `Handle` | `Bool` | |
| `__RTS_FN_NODE_TLS_SOCKET_AUTH_ERROR` | `Handle` | `StrPtr` | empty string if authorized |
| `__RTS_FN_NODE_TLS_SOCKET_GET_ALPN_PROTOCOL` | `Handle` | `StrPtr` | empty = none attempted; a synthetic sentinel distinguishes `false` (attempted, no match) from `null`/`undefined` (not attempted) — `.ts` shim maps the sentinel |
| `__RTS_FN_NODE_TLS_SOCKET_GET_SERVERNAME` | `Handle` | `StrPtr` | server-side; SNI hostname the client sent |
| `__RTS_FN_NODE_TLS_SERVER_NEW` | `Handle secureContext, StrPtr optionsJson` | `Handle` | |
| `__RTS_FN_NODE_TLS_SERVER_LISTEN` | `Handle, I32 port, StrPtr host, I32 backlog` | `Void` | binds + spawns the accept loop; each accepted plain socket is handed a `TlsAcceptor` handshake before `'secureConnection'` fires |
| `__RTS_FN_NODE_TLS_SERVER_CLOSE` | `Handle` | `Void` | |
| `__RTS_FN_NODE_TLS_SERVER_ADDRESS` | `Handle` | `StrPtr` (JSON `AddressInfo`) | |
| `__RTS_FN_NODE_TLS_SERVER_ADD_CONTEXT` | `Handle server, StrPtr hostname, Handle sniSecureContext` | `Void` | |
| `__RTS_FN_NODE_TLS_SERVER_SET_SECURE_CONTEXT` | `Handle server, Handle newSecureContext` | `Void` | does not affect already-established connections |
| `__RTS_FN_NODE_TLS_SERVER_GET_TICKET_KEYS` | `Handle` | `Handle` (Buffer, 48 bytes) | |
| `__RTS_FN_NODE_TLS_SERVER_SET_TICKET_KEYS` | `Handle, Handle keysBuf` | `Void` | |
| `__RTS_FN_NODE_TLS_SERVER_POLL_EVENT` | `Handle` | `Handle` (event record) or `0` | tag enum: `connection`/`secureConnection`/`tlsClientError`/`newSession`/`resumeSession`/`OCSPRequest`/`keylog` |
| `__RTS_FN_NODE_TLS_SERVER_NEW_SESSION_REPLY` | `Handle server, Handle requestToken` | `Void` | completes a pending `'newSession'` hook's `callback()` |
| `__RTS_FN_NODE_TLS_SERVER_RESUME_SESSION_REPLY` | `Handle server, Handle requestToken, Handle sessionDataBufOrZero` | `Void` | completes a pending `'resumeSession'` hook |
| `__RTS_FN_NODE_TLS_SERVER_OCSP_REPLY` | `Handle server, Handle requestToken, Handle responseBufOrZero, StrPtr errOrEmpty` | `Void` | completes a pending `'OCSPRequest'` hook |
| `__RTS_FN_NODE_TLS_CHECK_SERVER_IDENTITY` | `StrPtr hostname, StrPtr peerCertJson` | `StrPtr` | empty string = match; non-empty = error message. Reused verbatim by `node:https`'s own check (consolidation opportunity noted in §7 — `https.md`'s `__RTS_FN_NODE_HTTPS_CHECK_SERVER_IDENTITY` could delegate here instead of duplicating the algorithm) |
| `__RTS_FN_NODE_TLS_GET_CA_CERTIFICATES` | `StrPtr typeOrEmpty` | `StrPtr` (JSON `string[]` of PEM) | |
| `__RTS_FN_NODE_TLS_SET_DEFAULT_CA_CERTIFICATES` | `StrPtr pemArrayJson` | `Void` | |
| `__RTS_FN_NODE_TLS_GET_CIPHERS` | (none) | `StrPtr` (JSON `string[]`) | |
| `__RTS_FN_NODE_TLS_ROOT_CERTIFICATES` | (none) | `StrPtr` (JSON `string[]`) | backs the `tls.rootCertificates` frozen array; computed once, memoized `.ts`-side |

**Mutable module constants** (`DEFAULT_ECDH_CURVE`/`DEFAULT_MAX_VERSION`/`DEFAULT_MIN_VERSION`/`DEFAULT_CIPHERS`/`CLIENT_RENEG_LIMIT`/`CLIENT_RENEG_WINDOW`) are plain `.ts`-side mutable `let` bindings, **not** native constants — every native call that would consult them (`SECURE_CONTEXT_NEW`, `CONNECT`, `SERVER_NEW`) receives the *current* value serialized into its `optionsJson` argument at call time from the `.ts` shim, so a user reassigning `tls.DEFAULT_CIPHERS` before a later `connect()` call is observed correctly without any native-side global mutable state.

### 5.3 Async model

`node:tls` has no promise-returning API surface (`tls.connect`'s `callback` param is an event-listener shorthand, not an error-first Node callback, and there is no `tls.promises`). Async completion is delivered through the same **poll/drain typed-event-queue** model `node:http`/`node:https` use (see `https.md` §5.3), not the Promise subsystem:

- **Handshake** (`connect`, and each `Server` accept) runs on the shared multi-thread tokio runtime via `tokio_rustls::{TlsConnector, TlsAcceptor}` — never blocking an OS thread per connection.
- **Event delivery**: Rust pushes typed event records into a per-handle queue (`SOCKET_POLL_EVENT`/`SERVER_POLL_EVENT`); the `.ts` event-loop integration drains them each turn and re-emits real `EventEmitter` events (`'secureConnect'`, `'secureConnection'`, `'session'`, `'keylog'`, `'tlsClientError'`, etc.), preserving the ordering guarantees in §4.
- **Certificate verification** is synchronous CPU work (`rustls`'s chain validation) performed inline inside the handshake future — no separate thread pool needed.
- **User-overridable hooks that must round-trip** (`'newSession'`, `'resumeSession'`, `'OCSPRequest'`, `SNICallback`, `ALPNCallback`, `pskCallback`) all follow the same pattern: the handshake future reaches a point requiring the hook's answer, emits an event carrying a `requestToken` `Handle`, and **suspends** (does not fail the handshake) until the `.ts` shim calls the corresponding `*_REPLY` extern with that token. This is the same "await a queued user response mid-operation" shape `node:https`'s session-cache hooks already use.
- **`renegotiate()`** is synchronous from the JS caller's perspective in Node (returns `boolean` immediately, with the actual renegotiation — if any — happening async afterward and completing via callback); under RTS's `rustls` backend it returns `false` synchronously with no further async work (§4/§7).

### 5.4 Multithread / worker interaction

- **Single-owner-thread rule** (same as `node:http`/`node:https`, `https.md` §5.4): a `Server`/`TLSSocket`/in-flight handshake handle is used only from the RTS thread/region that created it; cross-thread use goes through a `channel` per `docs/specs/rts-threading-model.md`, never concurrent extern calls on the same handle from two threads.
- **`SecureContext` handles are the one piece of state that's cheap and safe to share across threads.** A built `rustls::{ClientConfig, ServerConfig}` pair is immutable and internally `Arc`-wrapped once constructed — many `TLSSocket`/`Server` handles across different RTS threads can reference the **same** `SecureContext` handle without needing shared-heap promotion (no interior-mutable per-connection state lives on the context itself). RTS's handle table should treat `SecureContext` as reference-counted-shared, not per-thread-copied — identical reasoning to `https.md` §5.4.
- **`ticketKeys` is the recommended cross-instance-sharing path for session-ticket resumption across a server farm/cluster** (per §4 — "all servers need same ticket keys"). This maps onto the RTS threading model as an explicit `shared` 48-byte buffer (`docs/specs/rts-threading-model.md`'s promotion-on-publication path): a `Buffer` obtained from `server.getTicketKeys()` on one thread, explicitly published as `shared`, and applied via `setTicketKeys()` on other threads/workers — this is a deliberate user action, not automatic propagation, matching the fact that Node itself requires the developer to wire this up manually across a `cluster`/multi-process deployment.
- **`newSession`/`resumeSession` custom session caches** are inherently single-server-instance-scoped unless the user's own hook implementation does cross-instance storage (e.g. writing to a shared cache) — RTS does not need to make the session-ID cache itself cross-thread-shared; the hook round-trip (§5.3) already runs on whatever thread owns the `Server` handle.
- **A listening `tls.Server` handed off to a `worker_threads`-style cluster** (fd-passing / accept-loop distribution across workers) is out of scope for this spec, same deferral as `node:http`/`node:https`.

### 5.5 Buffer / TypedArray interop

- Certificate/key/CA/CRL material (`cert`/`key`/`ca`/`crl`/`pfx` inputs) crosses as `StrPtr` when the caller supplied a PEM `string`, or as a `Buffer`/`ArrayBuffer` `Handle` when the caller supplied binary (`Buffer`, DER, or PKCS12 `pfx` bytes); the `.ts` shim normalizes whichever form the caller provided (string, `Buffer`, array of either) before invoking `SECURE_CONTEXT_NEW`, embedding string forms directly in the options JSON and passing binary forms as an accompanying `Handle` array.
- `ticketKeys`, `getTicketKeys()`/`setTicketKeys()`, `getSession()`/`getTLSTicket()`, `getFinished()`/`getPeerFinished()`, and `exportKeyingMaterial()` all return/accept `Buffer`-backed `Handle`s (the engine's `ArrayBuffer`/`Uint8Array` memory model — `Buffer extends Uint8Array` per the engine doctrine), never raw pointers across the ABI.
- `PeerCertificate.raw`/`pubkey` (and the recursive `issuerCertificate.raw` chain in `DetailedPeerCertificate`) are DER byte blobs; because they ride inside the single JSON `StrPtr` result for `getPeerCertificate()` (§5.2), they are base64-encoded in that JSON and reinflated to a `Buffer`/`Uint8Array` on the `.ts` side — same pattern `dns.md` uses for `TlsaRecord.data`.
- `ALPNProtocols` accepts `string[] | Buffer | TypedArray | DataView` per Node's typing; the `.ts` shim normalizes every form into a plain `string[]` before serializing into the options JSON, keeping the native side single-shaped (identical normalization choice to `https.md` §5.5).
- `keylog` lines are ASCII text (`StrPtr`), never binary.

### 5.6 Doctrine placement

- `tls` is **non-primordial** — no native literal/syntactic form; reached only via `import ... from "node:tls"`. The engine (`rts-codegen-new`) must never hardcode `"tls"` or any member name from this module.
- Resolution: `node:tls` → `rts-node`'s `NodespaceSpec { node_module: "tls", ns_prefix: "node_tls", members: TLS_MEMBERS }`, registered in `NODE_SPECS` (`crates/rts-node/src/lib.rs`) alongside every other node module. `ns_prefix_for("node:tls")` → `"node_tls"` (pure data lookup, no hardcoded arm); `node_lookup("node_tls.connect")` → the member's `symbol`/`args`/`returns` — identical mechanism to every other `node:` module already documented (`dns.md` §5.6, `https.md` §5.6).
- **Native-extern vs `.ts`-shim split**: every symbol in §5.2 is a raw primitive over opaque handles/JSON blobs (context construction, connect/listen, per-connection getters, event polling, hook-reply completion). All JS-shaped ergonomics — the `TLSSocket`/`Server`/`SecureContext` class wrappers, option-object normalization and defaulting (mutable `DEFAULT_*` constant substitution, `ALPNProtocols` form normalization, PEM-vs-Buffer input normalization), the three `tls.connect()` overload resolutions, the callback-as-one-shot-listener adapter, EventEmitter wiring off the poll/drain queue, and JSON decoding of `PeerCertificate`/`CipherNameAndProtocol`/etc into properly-shaped JS objects — live in a `.ts` shim shipped by `rts-node` (e.g. `rts-node/src/tls/{tls.ts, socket.ts, server.ts, secure_context.ts}`).
- `TLSSocket` is built by wrapping `node:net`'s plain-socket byte-transport primitives (read/write/pause/resume/end/destroy externs), not by reimplementing TCP — the TLS-specific externs in §5.2 are additive on top of whatever `node:net`'s `NodespaceSpec` already exposes for a `Socket` handle (see §5.7 — `node:net` does not have its own spec written yet in this repo, which is a phase-ordering prerequisite, not an rts-std dependency).

### 5.7 Shared-infra dependencies (FLAG)

- **Shared tokio runtime (`rt()`-equivalent accessor).** `tokio-rustls`'s `TlsConnector`/`TlsAcceptor` futures, and the accept-loop backing `tls.Server`, need a multi-thread async runtime. This currently lives as a global `OnceLock<Runtime>` under `rts-std`/`rts-runtime` (`crates/rts-runtime/src/runtime/async_rt.rs`). Since `rts-node` cannot depend on `rts-std`, this accessor must be reachable another way (hoisted into `rts-engine`, or a new shared low crate both `rts-primitives`/`rts-shared` and `rts-node` depend on) — or `rts-node` must own an entirely independent tokio runtime instance of its own. The latter avoids the hoist but means two multi-thread tokio runtimes coexist in one process (one for RTS's own `rts:*` async surface, one for every `node:*` async module) — a real tradeoff to resolve with an owner decision, flagged identically in `dns.md`/`https.md` §5.7 for every other async node module.
- **GC thread-registry hooks (`on_thread_start`/`on_thread_stop`).** Any tokio worker thread driving a TLS handshake or accept loop must be registered in `gc/thread_registry` so the GC's conservative stack scanner sees live handles (`TLSSocket`/`Server`/`SecureContext`/`Buffer` handles) held on that thread. This hook currently lives alongside the shared tokio runtime setup in `rts-std`/`rts-runtime` and needs the same reachability resolution as the runtime accessor above.
- **HandleTable-shaped slab.** `SecureContext`/`TLSSocket`/`Server`/session-ticket `Buffer` handles need a `HandleTable`-shaped slab (shard-aware, gen16+slot48 encoding) for GC-safe cross-thread reference. Prefer depending directly on `rts-engine::HandleTable` (the lowest layer, primordial-adjacent, not `rts-std`) rather than duplicating the shard logic in `rts-node` — same recommendation as `dns.md` §5.7.
- **`node:net`'s byte-transport primitives.** `TLSSocket` wraps a plain socket's raw read/write; `tls.Server` wraps `net.Server`'s accept loop. `node:net` does not yet have its own implementation spec in this repo (`docs/node-implementation/` has no `net.md` as of this writing) — this is an **in-crate `rts-node` sibling-module prerequisite**, not an `rts-std` dependency, but it blocks real implementation of the byte-I/O parts of this module until `node:net`'s own spec/implementation exists (or a minimal internal TCP transport is built directly inside `rts-node/tls` as an interim measure — see §5.8 phase (c)).
- **NOT flagged (explicitly, by design): `rustls`/`webpki-roots`/certificate-parsing crates.** Unlike the items above, the actual TLS cryptographic implementation is **not** something `rts-node` needs hoisted from `rts-std` — per the owner's independence decision, `rts-node` takes its **own direct Cargo dependency** on `rustls`, `tokio-rustls`, `webpki-roots`, `rustls-pemfile`, and `x509-parser`, deliberately duplicating (not sharing) what `rts-std`'s existing `tls`/`crypto` namespaces already vendor for RTS's own `rts:tls`/`rts:crypto` surface. This is a Cargo-dependency-level duplication, not a runtime-singleton hoist, so it carries none of the "unreachable without rts-std" problem the bullets above do.
- **Promise/settle subsystem — weaker dependency than in most async node modules.** Because `node:tls`'s own API surface is entirely event/callback-based (no `tls.promises`), it does not need the engine's Promise-create/settle machinery (`docs/specs/async-promise-function.md`) as directly as e.g. `node:dns`'s promise-mirrored surface does. It is still worth confirming whether the underlying host wake-up mechanism that drains the `.ts` event-loop's poll queue each turn shares any machinery with the Promise subsystem's microtask draining — flagged as a "verify degree of overlap" item rather than a hard blocker.

### 5.8 Implementation phases

1. **(a)** Add `rts-node/src/tls/mod.rs` with the `NodespaceSpec` skeleton (`node_module: "tls"`, `ns_prefix: "node_tls"`); register in `NODE_SPECS`.
2. **(b)** Implement the parts needing no networking/async infra first: `SecureContext` construction (`createSecureContext`, PEM/PKCS12/key parsing via `rustls-pemfile`+friends), `tls.getCiphers()`, `tls.rootCertificates`, `tls.getCACertificates`/`tls.setDefaultCACertificates`, `tls.checkServerIdentity` (pure function over a `PeerCertificate`-shaped JSON), and the six mutable `.ts`-side `DEFAULT_*`/`CLIENT_RENEG_*` bindings. All fully testable without a socket.
3. **(c)** Resolve the §5.7 blockers: decide + implement the tokio-runtime-accessor / GC-thread-registry-hook reachability strategy (hoist vs. `rts-node`-owned runtime), and confirm/implement enough of `node:net`'s byte-transport primitives (or a minimal interim internal TCP transport local to `rts-node/tls`) to build on. This is a Rule-C focus shift — a genuine blocker for every remaining phase, not busywork.
4. **(d)** Implement `tls.connect()` for the plain `{host, port}` / `{socket}` shapes: TCP connect (or reuse of an existing `node:net` socket handle) → `tokio_rustls::TlsConnector` handshake → `'secureConnect'`/`'error'` via the poll/drain queue. Establishes the event-queue + handle-lifecycle pattern reused by every later phase.
5. **(e)** Implement `tls.createServer()` + `tls.Server`: accept loop wrapping `node:net`'s server accept primitive with a `TlsAcceptor`, `'connection'` (pre-handshake) / `'secureConnection'` (post-handshake) / `'tlsClientError'` events, `addContext`/`SNICallback` dispatch, `setSecureContext`.
6. **(f)** Implement the `TLSSocket` instance getter surface: `getPeerCertificate`/`getCertificate`/`getCipher`/`getProtocol`/`isSessionReused`/`authorized`/`authorizationError`/`address`/`alpnProtocol`/`servername`.
7. **(g)** Implement ALPN (`ALPNProtocols`/`ALPNCallback`/`setKeyCert`) end-to-end, client and server.
8. **(h)** Implement session resumption: ticket-based first (`ticketKeys`/`sessionTimeout`/`getTicketKeys`/`setTicketKeys`, works automatically via `rustls`'s own ticketer once keys are set), then the `'newSession'`/`'resumeSession'` hook round-trip for session-ID-based custom caches, then the client-side `'session'` event (including the TLSv1.3 multiple-tickets case) and `getSession()`/`getTLSTicket()`.
9. **(i)** Implement OCSP stapling: server `'OCSPRequest'` hook round-trip, client `requestOCSP`/`'OCSPResponse'`.
10. **(j)** Implement the remaining `TLSSocket` diagnostic surface: `exportKeyingMaterial`, `getEphemeralKeyInfo`, `getFinished`/`getPeerFinished`, `getSharedSigalgs`, `setMaxSendFragment`, `enableTrace`, `getPeerX509Certificate`/`getX509Certificate` (delegating to `node:crypto`'s X509 construction).
11. **(k)** Implement `keylog` (`rustls::KeyLog` → per-handle line queue → `'keylog'` event, both `Server`-level "every connection" and `TLSSocket`-level "this connection").
12. **(l)** Implement the surface-compatible-but-inert `disableRenegotiation()`/`renegotiate()`/`CLIENT_RENEG_LIMIT`/`CLIENT_RENEG_WINDOW` stubs, documenting the `rustls`-imposed behavior divergence inline in the `.ts` shim's doc-comments.
13. **(m)** PSK support — best-effort, deferred until an owner decision on feasibility under `rustls` (§7); implement only if/when that decision lands.

## 6. Test plan

```
tests/node/tls/tls_secure_context.test.ts
  - tls.createSecureContext() with no options succeeds (uses bundled defaults)
  - tls.createSecureContext({ cert, key }) with a valid PEM pair succeeds
  - tls.createSecureContext({ key, cert, passphrase }) with an encrypted key succeeds; wrong passphrase throws
  - tls.createSecureContext({ pfx, passphrase }) succeeds with a valid PKCS12 bundle
  - tls.createSecureContext({ minVersion: 'TLSv1.3', maxVersion: 'TLSv1.2' }) throws ERR_TLS_PROTOCOL_VERSION_CONFLICT-shaped error
  - tls.createSecureContext({ ciphers: 'not-a-real-cipher' }) throws
  - tls.getCiphers() returns a non-empty array of lower-case strings
  - tls.rootCertificates is a non-empty frozen array; mutation attempt is a no-op/throws in strict mode
  - tls.getCACertificates('bundled') vs tls.getCACertificates('default') both return arrays
  - tls.setDefaultCACertificates([...]) then a fresh createSecureContext() with no ca picks it up

tests/node/tls/tls_connect_server_basic.test.ts
  - tls.createServer({ cert, key }, socket => socket.end()) + tls.connect({ port, rejectUnauthorized: false }) completes 'secureConnect'
  - server 'secureConnection' fires with a TLSSocket whose .encrypted === true
  - client socket.getProtocol() returns 'TLSv1.3' or 'TLSv1.2'
  - client socket.getCipher() returns {name, standardName, version}
  - self-signed server cert + client rejectUnauthorized: true (default) => client 'error' with a X.509 verify code (e.g. DEPTH_ZERO_SELF_SIGNED_CERT)
  - client-supplied ca matching the server's self-signed cert => authorized === true, no error

tests/node/tls/tls_sni.test.ts
  - server with addContext('a.example', ctxA) + addContext('b.example', ctxB); client connect with servername: 'a.example' gets ctxA's cert (assert via getPeerCertificate().subject)
  - server SNICallback(servername, cb) selecting per-hostname context dynamically
  - client with no servername set (path/socket-based connect) still completes handshake against the server's default context

tests/node/tls/tls_alpn.test.ts
  - server ALPNProtocols: ['h2','http/1.1'], client ALPNProtocols: ['http/1.1'] => negotiated 'http/1.1' on both sides
  - server ALPNCallback picks a protocol dynamically based on servername
  - no ALPN overlap => socket.alpnProtocol === false, connection still otherwise succeeds
  - setKeyCert() from within ALPNCallback switches served certificate (assert client sees the switched cert)

tests/node/tls/tls_session_resumption.test.ts
  - server with ticketKeys set; client connects twice, second connection's isSessionReused() === true
  - client captures 'session' event data, reconnects with { session: capturedBuffer }, resumes
  - TLSv1.3 connection: 'session' event fires more than once
  - getSession() returns undefined/empty on a TLSv1.3-only connection but real data on a TLSv1.2-forced connection
  - server.getTicketKeys()/setTicketKeys() round-trip (48-byte buffer) enables resumption against a second server instance sharing the same keys
  - server 'newSession'/'resumeSession' custom cache hooks fire and correctly restore a session by ID

tests/node/tls/tls_ocsp.test.ts
  - client requestOCSP: true + server 'OCSPRequest' hook replying with a canned response => client 'OCSPResponse' fires with matching bytes
  - server 'OCSPRequest' hook replying with an error => handshake still completes (stapling is best-effort, not mandatory)

tests/node/tls/tls_peer_certificate.test.ts
  - getPeerCertificate() shape: subject/issuer/valid_from/valid_to/serialNumber/fingerprint/fingerprint256/fingerprint512/raw
  - getPeerCertificate(true) includes issuerCertificate chain up to (but not including, or including per Node semantics — verify) the root
  - getPeerX509Certificate() returns a working node:crypto X509Certificate (cross-check .subject/.validTo against the plain getPeerCertificate() values)
  - tls.checkServerIdentity('wrong.example', cert) returns an Error with .reason/.host/.cert set for a cert not matching that hostname
  - tls.checkServerIdentity() ignores a uniformResourceIdentifier SAN entry even if the cert has one (CVE-2021-44531 regression guard)

tests/node/tls/tls_renegotiation_stubs.test.ts
  - socket.disableRenegotiation() does not throw
  - socket.renegotiate({}, cb) returns false and/or calls back with an error (document actual RTS behavior explicitly — see §7)
  - tls.CLIENT_RENEG_LIMIT / tls.CLIENT_RENEG_WINDOW are readable and writable numbers

tests/node/tls/tls_keylog.test.ts
  - server.on('keylog', (line, tlsSocket) => ...) receives Buffer lines in SSLKEYLOGFILE format for each accepted connection
  - client tlsSocket.on('keylog', line => ...) receives lines for its own connection only
  - no 'keylog' listener attached => no measurable overhead/queue growth (smoke-level assertion only)

tests/node/tls/tls_errors.test.ts
  - connecting to a non-TLS-speaking plain TCP port => client 'error' fires (not a hang), some ERR_SSL_*/ERR_TLS_*-shaped code
  - handshakeTimeout exceeded (server never responds) => 'tlsClientError'/client error with a timeout-shaped code
  - malformed cert/key passed to createSecureContext => synchronous throw, not a deferred error

tests/node/tls/tls_worker_threads.test.ts (multithread)
  - a SecureContext handle built on the main thread, explicitly shared via the threading model's `shared` mechanism, is usable to build TLSSocket connections concurrently from two different worker threads without corruption
  - ticketKeys buffer shared across two Server instances (simulating two worker "farm" members) enables session resumption across them
  - two independent tls.connect()/tls.createServer() pairs running concurrently on two different worker threads do not interfere (stress: N concurrent handshakes per worker, assert per-worker integrity)
```

## 7. Open questions / deferrals

- **DHE/`dhparam` has no `rustls` equivalent.** `rustls` implements only elliptic-curve (and, in recent versions, hybrid post-quantum) key exchange — no finite-field Diffie-Hellman at all. `dhparam: 'auto'`/explicit PEM params cannot be honored. This is an explicit, justified regression vs. real Node (per the "REGRESS WHEN NECESSARY" rule) that must be documented in the `.ts` shim's doc-comments and this file kept in sync — needs an owner decision on whether `createSecureContext({dhparam})` should silently ignore the option or throw a clear "unsupported" error.
- **Renegotiation is fundamentally unsupported by `rustls`.** `disableRenegotiation()`/`renegotiate()`/`CLIENT_RENEG_LIMIT`/`CLIENT_RENEG_WINDOW` can only be surface-compatible stubs (§4/§5.1/§5.2). Needs an owner decision on the exact stub behavior: does `renegotiate()` synchronously return `false` (Node's own documented behavior when renegotiation "cannot be initiated"), or invoke the callback with an `Error`? Node's docs are themselves a little ambiguous here; pick the interpretation that best matches real-world caller expectations (likely: return `false`, also call back with an error, matching the union of documented failure modes).
- **PSK (pre-shared key) feasibility under `rustls`.** Needs research/an owner decision: does the version of `rustls` RTS vendors support PSK-only or PSK-augmented cipher suites at all, and if so, does its API shape allow implementing Node's `pskCallback`/`pskIdentityHint` semantics acceptably? If not feasible, PSK should be explicitly deferred (not silently half-implemented) with a clear error at `createSecureContext`/`connect` time when a PSK cipher is requested.
- **`secureOptions` (OpenSSL `SSL_OP_*` bitmask) and `secureProtocol` (legacy method-name strings) have no `rustls` equivalent.** These are OpenSSL-internals-shaped escape hatches; best-effort mapping (e.g. recognizing `SSL_OP_NO_TICKET`-equivalent behavior via a `rustls` config flag where one exists) is plausible for a handful of common flags, but most of the bitmask space simply cannot be honored. Needs a scoping decision on which (if any) specific flags are worth special-casing versus documenting as unsupported wholesale.
- **`ERR_SSL_*` dynamically-named OpenSSL error codes** cannot be reproduced verbatim by a `rustls` backend (different error taxonomy entirely). RTS will need its own stable mapping from `rustls::Error` variants to Node-shaped `Error.code` strings — likely landing on the closest `ERR_TLS_*`/X.509-verify-result code rather than inventing new non-Node codes, but this needs a concrete mapping table built during implementation, not guessed here.
- **Session-resumption `Buffer` binary compatibility with a real Node/OpenSSL peer.** `getSession()`/`session` option blobs are `rustls`-internal serialization in RTS, not OpenSSL's session-ASN.1 format — resumption works RTS-to-RTS but a `Buffer` captured from a real Node process cannot be fed into an RTS `tls.connect({session})` (and vice versa). This is fine for RTS-to-RTS deployments (the overwhelmingly common case) but should be documented as a known non-goal, not silently assumed to work.
- **`node:net` prerequisite.** This spec assumes `TLSSocket`/`Server` can wrap `node:net`'s plain-socket/server accept-loop primitives; `node:net` has no implementation spec in this repo yet. Phase (c) (§5.8) either waits on that spec+implementation or builds a minimal interim TCP transport local to `rts-node/tls` — an implementation-order choice, not resolved here, mirroring the same open call `https.md` §7 leaves for `node:http`/`node:https`'s own relationship to `node:tls`.
- **Tokio-runtime hoist vs. `rts-node`-owned runtime (§5.7).** Whether `rts-node`'s async node modules (this one included) share the exact same tokio runtime instance as `rts-std`'s `rts:*` surface (requiring a hoist of the runtime accessor + GC thread-registry hooks into a lower shared crate) or run their own independent tokio runtime is a cross-cutting infra decision that affects every async `node:*` module, not just `node:tls` — flagged here so whoever implements this module doesn't make a one-off local choice that conflicts with `node:dns`/`node:fs/promises`/etc.
- **`getPeerCertificate(true)` chain depth semantics** (does the returned `issuerCertificate` chain include the trust-anchor root itself, or stop one level before it, matching real Node's exact behavior) needs verification against actual Node 25 behavior during implementation — the fetched documentation does not pin this down precisely.
- **Consolidating `checkServerIdentity` with `node:https`'s copy.** `https.md` §5.2 already specs a `__RTS_FN_NODE_HTTPS_CHECK_SERVER_IDENTITY` symbol; once this module exists, that should likely delegate to `__RTS_FN_NODE_TLS_CHECK_SERVER_IDENTITY` instead of maintaining two implementations of the same hostname-matching algorithm — flagged as a follow-up cleanup, not a blocker for either module individually.
