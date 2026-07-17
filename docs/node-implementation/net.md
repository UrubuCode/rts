# node:net

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:net` |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P1 |
| Status | [x] **Implemented** — `crates/rts-node/src/net/`. `isIP`/`isIPv4`/`isIPv6`, **`BlockList`**, **`SocketAddress`**, **`Server`** and **`Socket`** over real TCP (+ `createServer`/`connect`/`createConnection`, the Happy-Eyeballs racer and its config pair). NOT here: the `stream.Duplex` surface `Socket` inherits in Node (`pipe`/`read`/…) — `node:stream` does not exist yet; IPC (Unix sockets / named pipes) and `fd`/`onread`/`signal`/`lookup`, all REFUSED rather than faked. See §8. |
| Import forms | `import net from 'node:net'`; `import { Server, Socket, BlockList, SocketAddress, connect, createConnection, createServer, isIP, isIPv4, isIPv6 } from 'node:net'`; `const net = require('node:net')` |
| Globals exposed | none (all access is via the `node:net` module import; no ambient globals) |

## 1. Purpose

`node:net` provides the asynchronous, stream-based networking primitive that most of Node's higher-level networking (`node:http`, `node:tls`, `node:cluster`) is built on top of: TCP sockets/servers and Unix-domain/Windows-named-pipe IPC sockets/servers. It exposes `net.Socket` (a duplex stream wrapping a single connection, client or server-accepted) and `net.Server` (a listening socket that accepts connections and hands out `net.Socket` instances), plus `net.BlockList` (IP allow/deny-list rule sets usable by both client and server sockets) and `net.SocketAddress` (an immutable address/port/family value object). The module also exposes the family-autoselection ("Happy Eyeballs", RFC 8305 §5) dual-stack connection algorithm and simple IP-string classification (`isIP`/`isIPv4`/`isIPv6`).

## 2. Exported API surface (COMPLETE)

### Classes

#### `net.BlockList`

Added: v15.0.0, v14.18.0. Base class: none (plain class, not an `EventEmitter`). Events: none.

```typescript
class BlockList {
  constructor();

  addAddress(address: string | SocketAddress, type?: 'ipv4' | 'ipv6'): void;
  addRange(start: string | SocketAddress, end: string | SocketAddress, type?: 'ipv4' | 'ipv6'): void;
  addSubnet(net: string | SocketAddress, prefix: number, type?: 'ipv4' | 'ipv6'): void;
  check(address: string | SocketAddress, type?: 'ipv4' | 'ipv6'): boolean;

  // Stability: 1.2 - Release candidate (Added: v24.5.0, v22.19.0)
  fromJSON(value: string[] | string): void;
  toJSON(): string[];

  static isBlockList(value: unknown): value is BlockList; // Added: v23.4.0, v22.13.0

  readonly rules: string[];
}
```

| Method | Params | Optional/Default | Returns | Throws |
|---|---|---|---|---|
| `blockList.addAddress(address[, type])` | `address: string \| SocketAddress`; `type: 'ipv4'\|'ipv6'` | `type` optional, default `'ipv4'` | `undefined` | `ERR_INVALID_ARG_TYPE`, `ERR_INVALID_IP_ADDRESS` |
| `blockList.addRange(start, end[, type])` | `start, end: string \| SocketAddress`; `type` | `type` optional, default `'ipv4'` | `undefined` | `ERR_INVALID_ARG_TYPE`; throws if `start > end` |
| `blockList.addSubnet(net, prefix[, type])` | `net: string \| SocketAddress`; `prefix: number` (0-32 v4 / 0-128 v6); `type` | `type` optional, default `'ipv4'` | `undefined` | `ERR_OUT_OF_RANGE` on bad `prefix` |
| `blockList.check(address[, type])` | `address: string \| SocketAddress`; `type` | `type` optional, default `'ipv4'` | `boolean` | none (returns `false` on unparseable address) |
| `blockList.fromJSON(value)` | `value: string[] \| string` | required | `undefined` | `ERR_INVALID_ARG_TYPE` |
| `blockList.toJSON()` | — | — | `string[]` | none |
| `BlockList.isBlockList(value)` (static) | `value: unknown` | required | `boolean` | none |

Variant for every member: sync. Property `blockList.rules: string[]` — the accumulated list of rule strings, in insertion order. The format is **API**, not decoration (`toJSON()` emits it and `fromJSON()` parses it back), and it is exactly what Node's `Rule::ToString()` builds (`src/node_sockaddr.cc`, verified against the source):

| Rule | String |
|---|---|
| `addAddress('1.2.3.4')` | `"Address: IPv4 1.2.3.4"` |
| `addRange('1.2.3.0','1.2.3.255')` | `"Range: IPv4 1.2.3.0-1.2.3.255"` |
| `addSubnet('10.0.0.0', 8)` | `"Subnet: IPv4 10.0.0.0/8"` |

(An earlier draft of this spec guessed `"IPv4 range 1.2.3.0-1.2.3.255"` — wrong; corrected against Node's source when the class was implemented.)

#### `net.SocketAddress`

Added: v15.14.0, v14.18.0. Base class: none. Events: none.

```typescript
interface SocketAddressInitOptions {
  address?: string;   // default '127.0.0.1' (family 'ipv4') or '::' (family 'ipv6')
  family?: 'ipv4' | 'ipv6'; // default 'ipv4'
  flowlabel?: number; // only meaningful if family === 'ipv6'
  port?: number;
}

class SocketAddress {
  constructor(options?: SocketAddressInitOptions);

  static parse(input: string): SocketAddress | undefined; // Added: v23.4.0, v22.13.0

  readonly address: string;
  readonly family: 'ipv4' | 'ipv6';
  readonly flowlabel: number;
  readonly port: number;
}
```

| Member | Params | Returns | Variant |
|---|---|---|---|
| `new net.SocketAddress([options])` | `options?: SocketAddressInitOptions` | `SocketAddress` | sync |
| `SocketAddress.parse(input)` (static) | `input: string` — e.g. `'123.1.2.3:1234'` or `'[1::1]:1234'` | `SocketAddress \| undefined` (`undefined` on parse failure, never throws) | sync |
| `socketaddress.address` | — | `string` | property |
| `socketaddress.family` | — | `'ipv4' \| 'ipv6'` | property |
| `socketaddress.flowlabel` | — | `number` | property |
| `socketaddress.port` | — | `number` | property |

All properties are read-only (no setters) — a `SocketAddress` is an immutable value once constructed.

#### `net.Server`

**Extends:** `EventEmitter`.

```typescript
class Server extends EventEmitter {
  constructor(options?: ServerOptions, connectionListener?: (socket: Socket) => void);
  constructor(connectionListener?: (socket: Socket) => void);

  address(): AddressInfo | string | null;
  close(callback?: (err?: Error) => void): this;
  [Symbol.asyncDispose](): Promise<void>;
  getConnections(callback: (error: Error | null, count: number) => void): this;

  listen(handle: object, backlog?: number, callback?: () => void): this;
  listen(options: ListenOptions, callback?: () => void): this;
  listen(path: string, backlog?: number, callback?: () => void): this;
  listen(port?: number, host?: string, backlog?: number, callback?: () => void): this;

  ref(): this;
  unref(): this;

  listening: boolean;
  maxConnections: number;
  dropMaxConnection: boolean; // Added: v23.1.0, v22.12.0
}
```

| Method | Params table | Returns | Throws | Variant |
|---|---|---|---|---|
| `new net.Server([options][, connectionListener])` | `options: ServerOptions` (optional); `connectionListener: (socket: Socket) => void` (optional, auto-registered as one-time-per-connection `'connection'` listener — actually persistent, not one-time) | `Server` | none | sync |
| `server.address()` | — | `AddressInfo \| string \| null` (`null` before `'listening'`/after `close()`; `string` = pipe/socket path for IPC; object for TCP) | none | sync |
| `server.close([callback])` | `callback?: (err?: Error) => void` — called with `ERR_SERVER_NOT_RUNNING` if the server was not listening | `Server` (self, for chaining) | none synchronously | async completion, sync return |
| `server[Symbol.asyncDispose]()` | — | `Promise<void>` — resolves once the server has closed (enables `await using server = net.createServer()`) | rejects with same errors as `close()` | promise |
| `server.getConnections(callback)` | `callback: (error: Error \| null, count: number) => void` | `Server` | none | callback (Node docs mark it async: "Asynchronously get the number of concurrent connections") |
| `server.listen(handle[, backlog][, callback])` | `handle: {fd:number}\|Server\|Socket`; `backlog?: number`; `callback?: () => void` | `Server` | `ERR_SERVER_ALREADY_LISTEN` if already listening/closing | async (via `'listening'`/`'error'` events) |
| `server.listen(options[, callback])` | `options: ListenOptions`; `callback?: () => void` | `Server` | `ERR_SERVER_ALREADY_LISTEN`; `EADDRINUSE`/`EACCES`/etc via `'error'` event | async |
| `server.listen(path[, backlog][, callback])` | `path: string`; `backlog?: number`; `callback?: () => void` | `Server` | same as above | async |
| `server.listen([port[, host[, backlog]]][, callback])` | `port?: number` (0 = OS-assigned); `host?: string`; `backlog?: number`; `callback?: () => void` | `Server` | same as above | async |
| `server.ref()` | — | `Server` | none | sync |
| `server.unref()` | — | `Server` | none | sync |

Properties: `server.listening: boolean`; `server.maxConnections: number` (`0` drops all incoming connections since v21.0.0 — previously treated as `Infinity`); `server.dropMaxConnection: boolean` (Added v23.1.0/v22.12.0 — cluster-mode-only: forces closing instead of round-robining to another worker once `maxConnections` is hit).

Events (all inherited-`EventEmitter`-style, `server.on(name, cb)`):

| Event | Callback | Notes |
|---|---|---|
| `'close'` | `() => void` | server fully closed, no more connections |
| `'connection'` | `(socket: Socket) => void` | new inbound connection accepted |
| `'error'` | `(error: Error) => void` | `'close'` is **not** auto-emitted after this (unlike `Socket`) unless `close()` is called manually |
| `'listening'` | `() => void` | server bound and listening |
| `'drop'` (Added v18.6.0/v16.17.0) | `(data?: DropArgument) => void` | emitted instead of accepting a connection once `maxConnections` reached; `data` is `undefined` for non-TCP (IPC) servers |

#### `net.Socket`

**Extends:** `stream.Duplex` (which extends `EventEmitter`).

```typescript
class Socket extends stream.Duplex {
  constructor(options?: SocketConstructorOptions);

  address(): AddressInfo;

  connect(options: SocketConnectOpts, connectListener?: () => void): this;
  connect(path: string, connectListener?: () => void): this;
  connect(port: number, host?: string, connectListener?: () => void): this;

  destroy(error?: Error): this;
  destroySoon(): void;
  end(data?: string | Buffer | Uint8Array, encoding?: BufferEncoding, callback?: () => void): this;
  end(callback?: () => void): this;
  pause(): this;
  ref(): this;
  resetAndDestroy(): this; // Added: v18.3.0, v16.17.0
  resume(): this;
  setEncoding(encoding?: BufferEncoding): this;
  setKeepAlive(enable?: boolean, initialDelay?: number): this;
  setNoDelay(noDelay?: boolean): this;
  setTimeout(timeout: number, callback?: () => void): this;
  getTypeOfService(): number; // Added: v25.6.0, v24.15.0
  setTypeOfService(tos: number): this; // Added: v25.6.0, v24.15.0
  write(data: string | Buffer | Uint8Array, encoding?: BufferEncoding, callback?: (err?: Error) => void): boolean;

  readonly autoSelectFamilyAttemptedAddresses: string[];
  /** @deprecated since v14.6.0 — use writable.writableLength */
  bufferSize: number;
  readonly bytesRead: number;
  readonly bytesWritten: number;
  readonly connecting: boolean;
  readonly destroyed: boolean;
  readonly localAddress?: string;
  readonly localPort?: number;
  readonly localFamily?: string;
  readonly pending: boolean;
  readonly readyState: 'opening' | 'open' | 'readOnly' | 'writeOnly';
  readonly remoteAddress?: string;
  readonly remoteFamily?: string;
  readonly remotePort?: number;
  timeout?: number;
}
```

| Method | Params table | Returns | Throws | Variant |
|---|---|---|---|---|
| `new net.Socket([options])` | `options: SocketConstructorOptions` | `Socket` | `ERR_INVALID_FD_TYPE` if `fd` is not an int | sync |
| `socket.address()` | — | `{port:number, family:string, address:string}` | none | sync |
| `socket.connect(options[, connectListener])` | `options: SocketConnectOpts`; `connectListener?: () => void` (one-time `'connect'` listener) | `Socket` | `ERR_SOCKET_BAD_PORT`, `ERR_MISSING_ARGS`; connect failures via `'error'` event, not thrown | async |
| `socket.connect(path[, connectListener])` | `path: string`; `connectListener?: () => void` | `Socket` | same | async |
| `socket.connect(port[, host][, connectListener])` | `port: number`; `host?: string` (default `'localhost'`); `connectListener?: () => void` | `Socket` | `ERR_SOCKET_BAD_PORT` | async |
| `socket.destroy([error])` | `error?: Error` | `Socket` | none | sync trigger, async `'close'` |
| `socket.destroySoon()` | — | `void` | none | sync trigger (ends then destroys once flushed) |
| `socket.end([data[, encoding]][, callback])` | `data?: string\|Buffer\|Uint8Array`; `encoding?: BufferEncoding` (default `'utf8'`); `callback?: () => void` | `Socket` | none | sync trigger, async completion |
| `socket.pause()` | — | `Socket` | none | sync |
| `socket.ref()` | — | `Socket` | none | sync |
| `socket.resetAndDestroy()` | — | `Socket` | `ERR_SOCKET_CLOSED` if already destroyed | sync trigger (TCP `RST` only, no IPC) |
| `socket.resume()` | — | `Socket` | none | sync |
| `socket.setEncoding([encoding])` | `encoding?: BufferEncoding` | `Socket` | none | sync |
| `socket.setKeepAlive([enable][, initialDelay])` | `enable?: boolean` (default `false`); `initialDelay?: number` ms (default `0`) | `Socket` | none | sync |
| `socket.setNoDelay([noDelay])` | `noDelay?: boolean` (default `true`) | `Socket` | none | sync |
| `socket.setTimeout(timeout[, callback])` | `timeout: number` ms (`0` disables); `callback?: () => void` (one-time `'timeout'` listener) | `Socket` | none | sync |
| `socket.getTypeOfService()` | — | `number` (0-255) | `ERR_SOCKET_CLOSED` | sync |
| `socket.setTypeOfService(tos)` | `tos: number` (0-255) | `Socket` | `ERR_OUT_OF_RANGE`, `ERR_SOCKET_CLOSED` | sync |
| `socket.write(data[, encoding][, callback])` | `data: string\|Buffer\|Uint8Array`; `encoding?: BufferEncoding` (default `'utf8'`); `callback?: (err?) => void` | `boolean` (`true` fully flushed to kernel, `false` queued in user memory — `'drain'` fires when free) | none synchronously; write errors via `'error'` event | sync return, async flush |

Properties: `autoSelectFamilyAttemptedAddresses: string[]` (only present when family-autoselection is enabled; each entry `"$IP:$PORT"`); `bufferSize: number` (**deprecated v14.6.0**, use `writable.writableLength`); `bytesRead`/`bytesWritten: number`; `connecting: boolean`; `destroyed: boolean`; `localAddress?: string`, `localPort?: number`, `localFamily?: string`; `pending: boolean`; `readyState: 'opening'|'open'|'readOnly'|'writeOnly'`; `remoteAddress?/remoteFamily?/remotePort?`; `timeout?: number`.

Events:

| Event | Callback | Notes |
|---|---|---|
| `'close'` | `(hadError: boolean) => void` | `hadError` true iff closed due to a transmission error |
| `'connect'` | `() => void` | connection established |
| `'connectionAttempt'` (Added v21.6.0/v20.12.0) | `(ip: string, port: number, family: number) => void` | `family` is `4`\|`6`; may fire multiple times under `autoSelectFamily` |
| `'connectionAttemptFailed'` (Added v21.6.0/v20.12.0) | `(ip: string, port: number, family: number, error: Error) => void` | one per failed attempt |
| `'connectionAttemptTimeout'` (Added v21.6.0/v20.12.0) | `(ip: string, port: number, family: number) => void` | per-attempt timeout, only under `autoSelectFamily` |
| `'data'` | `(data: Buffer \| string) => void` | `string` only if `setEncoding()` was called |
| `'drain'` | `() => void` | write buffer emptied; throttle uploads on this |
| `'end'` | `() => void` | remote signaled FIN; with `allowHalfOpen:false` (default) the socket auto-ends its writable side after |
| `'error'` | `(error: Error) => void` | **always** immediately followed by `'close'` (unlike `Server`) |
| `'lookup'` | `(err: Error \| null, address: string, family: number \| string, host: string) => void` | fired after DNS resolution, before connecting |
| `'ready'` | `() => void` | fires immediately after `'connect'`; socket ready for read/write |
| `'timeout'` | `() => void` | idle timeout per `setTimeout()`; does **not** destroy the socket itself |

### Top-level functions

| Function | Variant |
|---|---|
| `net.connect(options[, connectListener])` | sync-return / async-connect |
| `net.connect(path[, connectListener])` | sync-return / async-connect |
| `net.connect(port[, host][, connectListener])` | sync-return / async-connect |
| `net.createConnection(options[, connectListener])` | sync-return / async-connect |
| `net.createConnection(path[, connectListener])` | sync-return / async-connect |
| `net.createConnection(port[, host][, connectListener])` | sync-return / async-connect |
| `net.createServer([options][, connectionListener])` | sync |
| `net.getDefaultAutoSelectFamily()` | sync |
| `net.setDefaultAutoSelectFamily(value)` | sync |
| `net.getDefaultAutoSelectFamilyAttemptTimeout()` | sync |
| `net.setDefaultAutoSelectFamilyAttemptTimeout(value)` | sync |
| `net.isIP(input)` | sync |
| `net.isIPv4(input)` | sync |
| `net.isIPv6(input)` | sync |

#### `net.connect(...)`

Pure alias of `net.createConnection(...)` — identical overloads, identical semantics; documented separately by Node only for discoverability.

#### `net.createConnection(options[, connectListener])`

| Name | Type | Optional | Default |
|---|---|---|---|
| `options` | `SocketConnectOpts & { timeout?: number }` (superset: same shape accepted by both `new Socket(options)` and `socket.connect(options)`, plus `timeout` which triggers `socket.setTimeout(timeout)` right after construction) | no | — |
| `connectListener` | `() => void` | yes | — (added as one-time `'connect'` listener) |

Returns: `Socket` (already `.connect()`-ing). Throws: `ERR_SOCKET_BAD_PORT`, `ERR_MISSING_ARGS` synchronously for malformed args; connection failures surface via `'error'`. Variant: sync return, async connect.

#### `net.createConnection(path[, connectListener])`

| Name | Type | Optional |
|---|---|---|
| `path` | `string` (IPC path) | no |
| `connectListener` | `() => void` | yes |

Returns: `Socket`. Variant: sync return, async connect.

#### `net.createConnection(port[, host][, connectListener])`

| Name | Type | Optional | Default |
|---|---|---|---|
| `port` | `number` | no | — |
| `host` | `string` | yes | `'localhost'` |
| `connectListener` | `() => void` | yes | — |

Returns: `Socket`. Variant: sync return, async connect.

#### `net.createServer([options][, connectionListener])`

| Name | Type | Optional | Default |
|---|---|---|---|
| `options` | `ServerOptions` | yes | `{}` |
| `connectionListener` | `(socket: Socket) => void` | yes | — (auto-registered as `'connection'` listener) |

Returns: `Server` (not yet listening — call `.listen()`). Throws: none synchronously for valid option shapes. Variant: sync.

#### `net.getDefaultAutoSelectFamily()`

Added: v19.4.0. No params. Returns: `boolean` — process-wide default for `Socket`'s `autoSelectFamily` option (default `true` since v20.0.0/v18.18.0). Variant: sync.

#### `net.setDefaultAutoSelectFamily(value)`

Added: v19.4.0.

| Name | Type | Optional |
|---|---|---|
| `value` | `boolean` | no |

Returns: `undefined`. Throws: `ERR_INVALID_ARG_TYPE` if not boolean. Variant: sync.

#### `net.getDefaultAutoSelectFamilyAttemptTimeout()`

Added: v19.8.0/v18.18.0 (paired with the attempt-timeout option). No params. Returns: `number` (ms; default `250`). Variant: sync.

#### `net.setDefaultAutoSelectFamilyAttemptTimeout(value)`

| Name | Type | Optional |
|---|---|---|
| `value` | `number` (values `< 10` are clamped up to `10`) | no |

Returns: `undefined`. Throws: `ERR_INVALID_ARG_TYPE`. Variant: sync.

#### `net.isIP(input)`

| Name | Type | Optional |
|---|---|---|
| `input` | `string` | no |

Returns: `number` — `6` if valid IPv6, `4` if valid IPv4, `0` otherwise. Throws: none (never throws; non-string coerces via `String(input)` first, per Node's loose typing here — treat as `(verify)` in the strict-TS RTS binding, which should require `string`). Variant: sync.

#### `net.isIPv4(input)`

| Name | Type | Optional |
|---|---|---|
| `input` | `string` | no |

Returns: `boolean`. Variant: sync.

#### `net.isIPv6(input)`

| Name | Type | Optional |
|---|---|---|
| `input` | `string` | no |

Returns: `boolean`. Variant: sync.

### Properties & constants

None. Unlike `node:dns`, `node:net` does not export module-level numeric/string constants (no `net.SOMAXCONN`-style export in current Node — historically some networking libs expose that but Node's `net` module does not); all configuration is via the option objects and class/function surface documented above. The well-known socket error codes (`ECONNREFUSED`, `EADDRINUSE`, `ECONNRESET`, `EPIPE`, `ETIMEDOUT`, …) are OS/`libuv` errno strings surfaced on `Error.code`, not module exports — see §4.

### Events

Consolidated (full per-event detail already given per-class above):

| Class | Events |
|---|---|
| `net.Server` | `close`, `connection`, `error`, `listening`, `drop` |
| `net.Socket` | `close`, `connect`, `connectionAttempt`, `connectionAttemptFailed`, `connectionAttemptTimeout`, `data`, `drain`, `end`, `error`, `lookup`, `ready`, `timeout` |
| `net.BlockList` | none |
| `net.SocketAddress` | none |

## 3. Types & option objects

```typescript
type BufferEncoding =
  | 'ascii' | 'utf8' | 'utf-8' | 'utf16le' | 'utf-16le'
  | 'ucs2' | 'ucs-2' | 'base64' | 'base64url' | 'latin1' | 'binary' | 'hex';

interface ServerOptions {
  allowHalfOpen?: boolean;         // default false
  blockList?: BlockList;           // disable INBOUND access to specific IPs
  highWaterMark?: number;          // default from stream.getDefaultHighWaterMark()
  keepAlive?: boolean;             // default false
  keepAliveInitialDelay?: number;  // default 0
  noDelay?: boolean;               // default false
  pauseOnConnect?: boolean;        // default false
}

interface AddressInfo {
  address: string;
  family: string; // 'IPv4' | 'IPv6'
  port: number;
}

// server.listen(options[, callback])
interface ListenOptions {
  backlog?: number;      // default 511 (not 512) — OS may cap via somaxconn/tcp_max_syn_backlog
  exclusive?: boolean;   // default false
  host?: string;
  ipv6Only?: boolean;    // default false; disables dual-stack for host '::'
  port?: number;
  path?: string;         // IPC; ignored if port is set
  reusePort?: boolean;   // default false; Linux 3.9+/DragonFlyBSD 3.6+/FreeBSD 12+/Solaris 11.4/AIX 7.2.5+ only
  readableAll?: boolean; // default false; IPC pipe world-readable
  writableAll?: boolean; // default false; IPC pipe world-writable
  signal?: AbortSignal;
}

interface DropArgument {
  localAddress: string;
  localPort: number;
  localFamily: string;
  remoteAddress: string;
  remotePort: number;
  remoteFamily: 'IPv4' | 'IPv6';
}

interface OnreadOptions {
  buffer: Buffer | Uint8Array | (() => Buffer | Uint8Array);
  callback: (bytesWritten: number, buf: Buffer | Uint8Array) => boolean; // return false to pause
}

interface SocketConstructorOptions {
  allowHalfOpen?: boolean;          // default false
  blockList?: BlockList;            // disable OUTBOUND access to specific IPs
  fd?: number;                      // wrap an existing fd
  keepAlive?: boolean;              // default false
  keepAliveInitialDelay?: number;   // default 0
  noDelay?: boolean;                // default false
  onread?: OnreadOptions;
  readable?: boolean;               // default false; only relevant with fd
  signal?: AbortSignal;
  typeOfService?: number;           // Added v25.6.0/v24.15.0; initial TOS/Traffic-Class value
  writable?: boolean;               // default false; only relevant with fd
}

type LookupFunction = (
  hostname: string,
  options: { family?: number; hints?: number; all?: boolean; verbatim?: boolean },
  callback: (err: NodeJS.ErrnoException | null, address: string | LookupAddress[], family?: number) => void,
) => void;

interface LookupAddress {
  address: string;
  family: number;
}

interface SocketConnectOpts {
  autoSelectFamily?: boolean;               // default net.getDefaultAutoSelectFamily()
  autoSelectFamilyAttemptTimeout?: number;  // default net.getDefaultAutoSelectFamilyAttemptTimeout(); values <10 clamp to 10
  family?: number;                          // 0 | 4 | 6; default 0
  hints?: number;                           // dns.ADDRCONFIG | dns.V4MAPPED | dns.ALL bitmask
  host?: string;                            // default 'localhost'
  localAddress?: string;
  localPort?: number;
  lookup?: LookupFunction;                  // default dns.lookup()
  port?: number;                            // required for TCP
  path?: string;                            // required for IPC; overrides TCP fields when present
}

interface SocketAddressInitOptions {
  address?: string;      // default '127.0.0.1' (ipv4) / '::' (ipv6)
  family?: 'ipv4' | 'ipv6'; // default 'ipv4'
  flowlabel?: number;    // ipv6 only
  port?: number;
}
```

## 4. Node semantics & edge cases

- **IPC path conventions differ per OS.** On **Unix**, the "local domain" is a filesystem path (`AF_UNIX`); length is capped by `sizeof(sockaddr_un.sun_path)` (typically 107 bytes Linux, 103 bytes macOS) — exceeding it throws synchronously. A Unix domain socket created by a Node.js API (e.g. via `server.listen(path)`) is `unlink()`-ed by `server.close()`; a socket the caller created out-of-band, or left behind by a crash, is **not** auto-removed and stays visible in the filesystem until manually unlinked. **Linux abstract sockets** (`\0` + name, e.g. `\0abstract`, added v20.8.0/pre-v20.4.0 needed literal `\0`) are invisible in the filesystem and vanish automatically when the last reference closes. On **Windows**, IPC is a named pipe; the path *must* be under `\\?\pipe\` or `\\.\pipe\` (the latter may resolve `..` segments); the pipe namespace is flat (no real subdirectories) and pipes never persist past the last open reference — Windows removes them automatically even if the owning process crashes (unlike Unix sockets). JS string literals need extra backslash-escaping for these prefixes.
- **Happy Eyeballs / `autoSelectFamily`** loosely implements RFC 8305 §5: `lookup(host, {all:true})` resolves every A/AAAA record; the algorithm tries the first AAAA, then the first A, then the second AAAA, etc., each attempt capped by `autoSelectFamilyAttemptTimeout` (values `<10` clamp to `10`) before moving to the next candidate — except the *last* attempt, which is not time-boxed the same way. Only engages when `family` is `0` and `localAddress` is unset. Individual connection errors are swallowed as long as **any** attempt succeeds; if **all** attempts fail, a single `AggregateError` wrapping every per-attempt error is what surfaces on the socket's `'error'` event. Default flipped to `true` in v20.0.0/v18.18.0 (was opt-in before). Emits `'connectionAttempt'`/`'connectionAttemptFailed'`/`'connectionAttemptTimeout'` per candidate (v21.6.0/v20.12.0), and populates `socket.autoSelectFamilyAttemptedAddresses` (only present when the algorithm actually ran).
- **`write()` backpressure.** Returns `true` if the OS accepted the entire payload into the kernel socket buffer immediately; `false` if some/all of it is queued in the process's own memory — a `'drain'` event fires once that internal queue empties. RTS's Socket, like Node's, must let `write()` "always work" (never block/reject on backlog) — the caller is expected to `pause()`/wait for `'drain'` to avoid unbounded memory growth. `bufferSize` (deprecated since v14.6.0, in favor of `writable.writableLength`) is only an *approximation* since buffered string lengths in bytes aren't known until encoded.
- **`allowHalfOpen`.** Default `false`: on receiving the remote's FIN (`'end'` fires), the socket auto-sends its own FIN and destroys the fd once its pending write queue drains (RFC 1122 §4.2.2.13 half-close). With `allowHalfOpen: true`, the writable side stays open indefinitely after `'end'`; the app must call `end()` explicitly to close for real. Applies to both `net.Socket` (client) and `net.Server`/`net.createServer` (inherited by accepted sockets).
- **`socket.readyState`** is a derived string, not stored state: `'opening'` while connecting, `'open'` when both readable+writable, `'readOnly'` when only readable (write side ended), `'writeOnly'` when only writable (read side ended).
- **`EADDRINUSE` is the canonical retry-driving error** on `server.listen()` — Node's own doc pattern is: listen on `'error'`, and on `EADDRINUSE` call `server.close()` then retry `server.listen(...)` after a delay. `server.listen()` may be called again **only** after an error on the first attempt or after `server.close()` — otherwise it throws `ERR_SERVER_ALREADY_LISTEN` synchronously.
- **`backlog` default is 511, not 512** — actual queue length is still OS-governed (`somaxconn`, `tcp_max_syn_backlog` on Linux); RTS must not silently clamp/round this.
- **`SO_REUSEADDR`** is set on every `net.Socket`/listening socket by default (matches Node/libuv). **`reusePort`** (v23.1.0/v22.12.0) is a distinct, narrower `SO_REUSEPORT` opt-in limited to Linux 3.9+/DragonFlyBSD 3.6+/FreeBSD 12.0+/Solaris 11.4/AIX 7.2.5+ — unsupported platforms must raise an error, not silently ignore the option.
- **Dual-stack (`::`) binding.** Omitting `host` binds `::` (unspecified IPv6) when IPv6 is available, else `0.0.0.0`. On most OSes, binding `::` *also* accepts IPv4 traffic (mapped addresses) unless `ipv6Only: true` explicitly disables that dual-stack behavior — Windows and some BSDs may differ subtly here; treat as platform-variable and surface `ipv6Only` faithfully rather than hardcoding a single OS's behavior.
- **`maxConnections`.** `0` drops *all* incoming connections (changed in v21.0.0 — previously `0` meant `Infinity`, an easy Node-version-skew trap). Non-cluster mode: reaching the threshold closes the new connection outright and emits `'drop'`. Cluster mode: by default the connection is instead routed to another worker; `server.dropMaxConnection = true` forces close-instead-of-route (cluster-only; not meaningful for a single-process RTS server unless/until an RTS cluster-equivalent exists).
- **`'error'` vs `'close'` ordering differs by class.** On `Socket`, `'error'` is *always* immediately followed by `'close'`. On `Server`, `'error'` is emitted **without** an automatic `'close'` unless the app calls `close()` itself — this asymmetry is a common Node footgun and must be preserved exactly (not "helpfully" auto-closing the server on error).
- **`socket.connect()` reconnection restriction.** Calling `connect()` again on a socket is only supported after a `'close'` event; doing so at other times is documented as leading to undefined behavior — RTS should mirror Node's actual behavior (whatever it concretely does) rather than inventing stricter validation, to stay byte-for-byte compatible with existing Node programs that rely on the (fragile) real behavior.
- **`BlockList` is not a security boundary against proxies/NAT.** Explicitly documented: it does not work "if the server is behind a reverse proxy, NAT, etc." because the address checked is the proxy's/NAT's address, not the true originating client — RTS docs/tests must not oversell it as a security feature beyond this scope.
- **IPC permission bits (`readableAll`/`writableAll`).** Starting an IPC server as root can leave the pipe/socket inaccessible to unprivileged users; these two `listen()` options exist specifically to relax that (world-readable/writable). Security-sensitive default is `false` for both.
- **Deprecations.** `socket.bufferSize` deprecated since v14.6.0 (stability index 0) — implement it as a computed alias over `writable.writableLength` rather than a genuinely separate counter. No other hard-removed APIs in this module as of Node 25.
- **Error codes surfaced on `Error.code`** (OS/libuv errno strings, not module constants): `ECONNREFUSED`, `ECONNRESET`, `EADDRINUSE`, `EADDRNOTAVAIL`, `EAFNOSUPPORT`, `EPIPE`, `ETIMEDOUT`, `EHOSTUNREACH`, `ENETUNREACH`, `EACCES`, `EMFILE`/`ENFILE` (fd exhaustion), `ENOTFOUND` (from an internal `dns.lookup` during `connect(host,port)`). Node-specific (non-errno) codes: `ERR_SERVER_ALREADY_LISTEN`, `ERR_SERVER_NOT_RUNNING`, `ERR_SOCKET_CLOSED`, `ERR_SOCKET_BAD_PORT`, `ERR_INVALID_ARG_TYPE`, `ERR_INVALID_IP_ADDRESS`, `ERR_MISSING_ARGS`, `ERR_INVALID_FD_TYPE`, `ERR_OUT_OF_RANGE`.
- **`server.listen({signal})`** — aborting the associated `AbortController` is equivalent to calling `server.close()`.
- **No backpressure at the `Server` accept level** beyond `maxConnections`/OS backlog — accepted sockets themselves carry the usual `Socket` write-backpressure semantics.
- **Windows vs POSIX summary:** named pipes (no persistence, flat namespace, auto-cleanup on last-handle-close) vs Unix domain sockets (filesystem-visible, persist until unlinked, abstract-namespace escape hatch on Linux only); `reusePort` platform-gated (no Windows support); backlog/somaxconn tuning is a POSIX-specific knob RTS can't fully replicate on Windows (Windows has its own `listen()` backlog cap, generally smaller / differently governed).

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node` is a fully independent crate — no `rts-std` dependency, no reuse of `rts-std`'s existing (sync, `std::net`-based) `net`/`tls` namespaces. `node:net` needs genuinely async, event-driven multiplexed I/O (accept loops, non-blocking reads feeding `'data'`, write-queue backpressure feeding `'drain'`), which the old `rts-std::net` (blocking `std::net::TcpListener`/`TcpStream`) does not provide — so this module is built fresh on an async foundation:

- **TCP.** `tokio::net::{TcpListener, TcpStream}` for the actual accept/connect/read/write. Socket-option knobs `tokio::net::TcpStream` doesn't expose directly (`SO_REUSEADDR`/`SO_REUSEPORT`, custom `backlog` at `listen(2)` time, TOS/Traffic-Class, keepalive interval) go through the `socket2` crate: build a `socket2::Socket` (bind + `set_reuse_address`/`set_reuse_port` + `listen(backlog)`), then `TcpListener::from_std(socket.into())` to hand it to tokio; `socket2::SockRef::from(&tokio_stream)` gives the same setsockopt access on an already-async `TcpStream` without taking ownership (for `setKeepAlive`/`setNoDelay`/`getTypeOfService`/`setTypeOfService`).
- **Unix-domain IPC (POSIX).** `tokio::net::{UnixListener, UnixStream}`. Regular paths bind directly; the Linux abstract-namespace convention (leading `\0`) is handled via `std::os::unix::net::SocketAddr::from_abstract_name` (Linux-only in std) with a raw `libc::bind` + manually-built `sockaddr_un` fallback for correctness on older toolchains/other platforms that lack the std helper.
- **Windows named-pipe IPC.** `tokio::net::windows::named_pipe::{NamedPipeServer, NamedPipeClient, ServerOptions, ClientOptions}` behind `#[cfg(windows)]`, selected whenever the `.ts` shim detects a `\\?\pipe\`/`\\.\pipe\`-prefixed path (mirrors Node's own OS branch — RTS's `Server`/`Socket` native layer picks TCP vs Unix-socket vs named-pipe backing purely from the path/port shape passed down, exactly like Node does internally).
- **`BlockList`.** A small in-crate rule engine (`rts-node/src/net/blocklist.rs`): three rule kinds — `Address(IpAddr)`, `Range(IpAddr, IpAddr)`, `Subnet(IpAddr, u8 prefix)` — each tagged v4/v6; `check()` linear-scans the rule vec (small N in practice; Node itself is not a fancy trie either for the common case). No external crate needed — CIDR containment is a few bitwise ops over the 4-byte/16-byte address representation `std::net::Ipv4Addr`/`Ipv6Addr` already expose (`.octets()`).
- **`SocketAddress`.** A plain immutable value struct `{address: IpAddr, family, port: u16, flowlabel: u32}`; `parse()` is a small manual splitter (`"ip:port"` / `"[ipv6]:port"`) feeding `IpAddr::from_str` — no need for a URL/URI crate.
- **`isIP`/`isIPv4`/`isIPv6`.** Pure `std::net::IpAddr::from_str`/`Ipv4Addr::from_str`/`Ipv6Addr::from_str` classification; zero I/O.
- **Happy Eyeballs (`autoSelectFamily`).** A custom racer over the resolved-address list (AAAA-first-then-A ordering, per §4) built with `tokio::time::timeout` per attempt + a manual sequential-with-early-return loop (not literally RFC 8305's concurrent-fan-out; Node's own implementation is also sequential-with-timeout, not truly parallel — match Node's actual behavior, not the RFC's ideal). Requires the same OS-hostname-resolution primitive `node:dns`'s `lookup()` uses (see 5.7 — this is a real cross-module dependency, not incidental).
- **Encodings for `write()`/`'data'`/`setEncoding()`.** Reuse the same UTF-8/ASCII/Latin1/hex/base64/UTF-16LE conversion helpers other `rts-node` modules need (buffer-encoding is a cross-cutting concern — likely belongs in a small shared `rts-node` internal `encoding.rs`, not reinvented per module).

### 5.2 ABI surface

Symbol convention: `__RTS_FN_NODE_NET_<NAME>`. Rich/stateful objects (`Server`, `Socket`, `BlockList`) are opaque `Handle` (u64) values into an `rts-node`-owned handle slab. `SocketAddress` is a small **immutable value type** with no mutation and no events — rather than a `Handle`, it crosses the ABI as a flat 4-tuple (`StrPtr address, I32 family, I32 port, I32 flowlabel`) or a single JSON `StrPtr`; the `.ts` shim normalizes any `SocketAddress` instance passed *into* a `BlockList`/`connect()` call down to its plain string+family fields before calling native code, so no native entry point ever needs to accept a `SocketAddress` handle as input.

| Symbol | Args (AbiType) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_NET_IS_IP` | `StrPtr input` | `I32` (0\|4\|6) | sync, no I/O |
| `__RTS_FN_NODE_NET_IS_IPV4` | `StrPtr input` | `Bool` | sync |
| `__RTS_FN_NODE_NET_IS_IPV6` | `StrPtr input` | `Bool` | sync |
| `__RTS_FN_NODE_NET_SOCKET_ADDRESS_PARSE` | `StrPtr input` | `StrPtr` (JSON `{address,family,port,flowlabel}` or empty sentinel on parse failure) | sync |
| `__RTS_FN_NODE_NET_GET_DEFAULT_AUTO_SELECT_FAMILY` | (none) | `Bool` | sync, process-wide config |
| `__RTS_FN_NODE_NET_SET_DEFAULT_AUTO_SELECT_FAMILY` | `Bool value` | `Void` | sync |
| `__RTS_FN_NODE_NET_GET_DEFAULT_AUTO_SELECT_FAMILY_ATTEMPT_TIMEOUT` | (none) | `I64` (ms) | sync |
| `__RTS_FN_NODE_NET_SET_DEFAULT_AUTO_SELECT_FAMILY_ATTEMPT_TIMEOUT` | `I64 valueMs` | `Void` | sync; clamps `<10` up to `10` |
| `__RTS_FN_NODE_NET_BLOCKLIST_NEW` | (none) | `Handle` | |
| `__RTS_FN_NODE_NET_BLOCKLIST_ADD_ADDRESS` | `Handle bl, StrPtr address, StrPtr type` | `Void` | throws via thread-local error slot on bad address |
| `__RTS_FN_NODE_NET_BLOCKLIST_ADD_RANGE` | `Handle bl, StrPtr start, StrPtr end, StrPtr type` | `Void` | |
| `__RTS_FN_NODE_NET_BLOCKLIST_ADD_SUBNET` | `Handle bl, StrPtr net, I32 prefix, StrPtr type` | `Void` | |
| `__RTS_FN_NODE_NET_BLOCKLIST_CHECK` | `Handle bl, StrPtr address, StrPtr type` | `Bool` | never throws (invalid address ⇒ `false`) |
| `__RTS_FN_NODE_NET_BLOCKLIST_TO_JSON` | `Handle bl` | `StrPtr` (JSON `string[]`) | |
| `__RTS_FN_NODE_NET_BLOCKLIST_FROM_JSON` | `Handle bl, StrPtr rulesJson` | `Void` | mutates rule set in place |
| `__RTS_FN_NODE_NET_BLOCKLIST_FREE` | `Handle bl` | `Void` | |
| `__RTS_FN_NODE_NET_SERVER_NEW` | `StrPtr optionsJson` | `Handle` | `optionsJson` carries `allowHalfOpen/blockList-handle-id/highWaterMark/keepAlive/keepAliveInitialDelay/noDelay/pauseOnConnect` |
| `__RTS_FN_NODE_NET_SERVER_LISTEN` | `Handle server, StrPtr listenOptionsJson` | `Void` | async; completion/failure delivered via the `'listening'`/`'error'` callback slots registered below, not a return value |
| `__RTS_FN_NODE_NET_SERVER_CLOSE` | `Handle server` | `Void` | async; completion via `'close'` callback slot |
| `__RTS_FN_NODE_NET_SERVER_ADDRESS` | `Handle server` | `StrPtr` (JSON `AddressInfo`, JSON string for a path, or empty sentinel for `null`) | sync |
| `__RTS_FN_NODE_NET_SERVER_GET_CONNECTIONS` | `Handle server` | `I64` | sync count read; `.ts` shim defers the callback via a microtask to match Node's async-shaped contract |
| `__RTS_FN_NODE_NET_SERVER_REF` / `_UNREF` | `Handle server` | `Void` | sync |
| `__RTS_FN_NODE_NET_SERVER_IS_LISTENING` | `Handle server` | `Bool` | sync |
| `__RTS_FN_NODE_NET_SERVER_GET_MAX_CONNECTIONS` / `_SET_MAX_CONNECTIONS` | `Handle server[, I64 value]` | `I64` / `Void` | sync |
| `__RTS_FN_NODE_NET_SERVER_GET_DROP_MAX_CONNECTION` / `_SET_DROP_MAX_CONNECTION` | `Handle server[, Bool value]` | `Bool` / `Void` | sync |
| `__RTS_FN_NODE_NET_SERVER_ON_CONNECTION` | `Handle server, U64 fnPtr` | `Void` | registers the native accept-loop callback; see 5.3 |
| `__RTS_FN_NODE_NET_SERVER_ON_ERROR` / `_ON_LISTENING` / `_ON_CLOSE` / `_ON_DROP` | `Handle server, U64 fnPtr` | `Void` | one native callback slot per event kind |
| `__RTS_FN_NODE_NET_SOCKET_NEW` | `StrPtr optionsJson` | `Handle` | `fd`/`onread`/`typeOfService`/etc from `SocketConstructorOptions` |
| `__RTS_FN_NODE_NET_SOCKET_CONNECT` | `Handle socket, StrPtr connectOptionsJson` | `Void` | async; drives the Happy-Eyeballs racer internally when `autoSelectFamily` is set |
| `__RTS_FN_NODE_NET_SOCKET_WRITE_STR` | `Handle socket, StrPtr data, StrPtr encoding` | `Bool` | text path |
| `__RTS_FN_NODE_NET_SOCKET_WRITE_BYTES` | `Handle socket, U64 ptr, U64 len` | `Bool` | raw `Buffer`/`Uint8Array`/`ArrayBuffer` path — no copy/encoding, see 5.5 |
| `__RTS_FN_NODE_NET_SOCKET_END` | `Handle socket, StrPtr data, StrPtr encoding` | `Void` | empty `data` ⇒ end with no final write |
| `__RTS_FN_NODE_NET_SOCKET_DESTROY` | `Handle socket, StrPtr errorMessage` | `Void` | empty message ⇒ clean destroy |
| `__RTS_FN_NODE_NET_SOCKET_DESTROY_SOON` | `Handle socket` | `Void` | |
| `__RTS_FN_NODE_NET_SOCKET_PAUSE` / `_RESUME` | `Handle socket` | `Void` | |
| `__RTS_FN_NODE_NET_SOCKET_SET_ENCODING` | `Handle socket, StrPtr encoding` | `Void` | |
| `__RTS_FN_NODE_NET_SOCKET_SET_KEEP_ALIVE` | `Handle socket, Bool enable, I64 initialDelayMs` | `Void` | |
| `__RTS_FN_NODE_NET_SOCKET_SET_NO_DELAY` | `Handle socket, Bool noDelay` | `Void` | |
| `__RTS_FN_NODE_NET_SOCKET_SET_TIMEOUT` | `Handle socket, I64 timeoutMs` | `Void` | one-time-callback registration is a separate `_ON_TIMEOUT` slot call from `.ts` |
| `__RTS_FN_NODE_NET_SOCKET_RESET_AND_DESTROY` | `Handle socket` | `Void` | TCP only |
| `__RTS_FN_NODE_NET_SOCKET_REF` / `_UNREF` | `Handle socket` | `Void` | |
| `__RTS_FN_NODE_NET_SOCKET_ADDRESS` | `Handle socket` | `StrPtr` (JSON) | |
| `__RTS_FN_NODE_NET_SOCKET_GET_TYPE_OF_SERVICE` | `Handle socket` | `I32` | |
| `__RTS_FN_NODE_NET_SOCKET_SET_TYPE_OF_SERVICE` | `Handle socket, I32 tos` | `Void` | |
| `__RTS_FN_NODE_NET_SOCKET_GET_BYTES_READ` / `_GET_BYTES_WRITTEN` | `Handle socket` | `I64` | |
| `__RTS_FN_NODE_NET_SOCKET_IS_CONNECTING` / `_IS_DESTROYED` / `_IS_PENDING` | `Handle socket` | `Bool` | |
| `__RTS_FN_NODE_NET_SOCKET_GET_LOCAL_ADDRESS` / `_GET_LOCAL_FAMILY` / `_GET_REMOTE_ADDRESS` / `_GET_REMOTE_FAMILY` / `_GET_READY_STATE` | `Handle socket` | `StrPtr` | empty sentinel where the Node property is `undefined` (not-yet-connected) |
| `__RTS_FN_NODE_NET_SOCKET_GET_LOCAL_PORT` / `_GET_REMOTE_PORT` / `_GET_TIMEOUT` | `Handle socket` | `I64` | `-1` sentinel for `undefined` |
| `__RTS_FN_NODE_NET_SOCKET_GET_AUTO_SELECT_FAMILY_ATTEMPTED_ADDRESSES` | `Handle socket` | `StrPtr` (JSON `string[]`) | empty array if autoselect never ran |
| `__RTS_FN_NODE_NET_SOCKET_ON_DATA` / `_ON_CONNECT` / `_ON_CLOSE` / `_ON_DRAIN` / `_ON_END` / `_ON_ERROR` / `_ON_LOOKUP` / `_ON_READY` / `_ON_TIMEOUT` / `_ON_CONNECTION_ATTEMPT` / `_ON_CONNECTION_ATTEMPT_FAILED` / `_ON_CONNECTION_ATTEMPT_TIMEOUT` | `Handle socket, U64 fnPtr` | `Void` | one native callback slot per event kind |
| `__RTS_FN_NODE_NET_SOCKET_FREE` / `__RTS_FN_NODE_NET_SERVER_FREE` | `Handle` | `Void` | drops the underlying tokio resource |

Event registration takes a raw `U64 fnPtr` (a first-class Cranelift function pointer the engine already materializes for any identifier resolving to a user function — see `05-codegen-notes.md`, "First-class function pointers") rather than a full `Function`-class `Handle`. This is a deliberate scope-reduction: `node:net`'s internal accept/read loops only ever need to *invoke* the registered JS callback with a fixed, small argument list (e.g. the new `Socket` handle for `'connection'`, or nothing for `'listening'`) — they do not need `.bind()`/`.call()`/`this`-rebinding semantics, so there is no need to route through the engine's `Entry::Function`/`invoke_n` machinery at all, which keeps this module from acquiring a dependency on that (currently `rts-shared`/`rts-runtime`-side) subsystem. The `.ts` shim is responsible for wrapping arbitrary user closures (which may capture `this`/outer scope) into a plain top-level trampoline function before handing its pointer to the native `_ON_*` call.

### 5.3 Async model

- **Accept loop (`Server`).** `SERVER_LISTEN` spawns a `tokio::task` running `loop { listener.accept().await; … }` on the shared tokio runtime; each accepted connection allocates a new `Socket` handle and invokes the registered `'connection'` callback pointer directly from that task (per 5.2, a raw fn-pointer call, not a full Promise-settle round-trip — `'connection'` is a plain synchronous-from-JS's-perspective event, not a Promise). Listen-failure (bind/listen `Result::Err`) invokes the `'error'` callback pointer instead of using the promise subsystem, matching `Server`'s "no implicit `'close'` after `'error'`" semantics from §4.
- **Connect (`Socket.connect`).** Single-family case: spawn a tokio task doing `TcpStream::connect`/`UnixStream::connect`/named-pipe-connect; on success, invoke `'connect'` then `'ready'` callback pointers and start the read loop (below); on failure, invoke `'error'` then `'close'` (always paired, per §4). `autoSelectFamily` case: the Happy-Eyeballs racer (5.1) drives the same per-attempt `'connectionAttempt'`/`'connectionAttemptFailed'`/`'connectionAttemptTimeout'` callback pointers as it goes, then falls into the same success/failure path once a winner is chosen or all attempts are exhausted (`AggregateError` synthesized `.ts`-side from the collected per-attempt errors passed back as a JSON array).
- **Read loop (`Socket`, established connection).** A tokio task running `loop { stream.read(&mut buf).await; … }`; each chunk invokes the `'data'` callback pointer with a freshly-allocated `Uint8Array`/`Buffer`-backed chunk (or, if `setEncoding()` was called, a decoded `StrPtr`). EOF (`read()` returns 0) invokes `'end'`, then (per `allowHalfOpen`) either auto-drives a native `end()`/half-close or leaves the writable side open. The task is paused while `pause()`-flagged (a shared `AtomicBool`/tokio `Notify` the reader loop checks) and resumes on `resume()`.
- **Write path (`Socket.write`/`.end`).** Native call attempts a non-blocking write via the tokio stream; if the OS accepts everything, returns `true` immediately; otherwise the remainder is queued in an internal `VecDeque<u8>` and a background flush task drains it, invoking `'drain'` once empty — mirroring Node's "write always works, backpressure is advisory" contract from §4.
- **`getConnections`, `address`, `isIP*`, `BlockList.*`, `SocketAddress.*`, ref/unref, encoding/keepAlive/noDelay/timeout/TOS setters** are synchronous native calls with no tokio/promise involvement (plain field reads/`setsockopt`-equivalent calls), consistent with their `sync` variant tag in §2.
- **`server[Symbol.asyncDispose]()`** is the one genuinely Promise-shaped surface in this module — it needs the promise-create/settle machinery (see 5.7) to resolve once the native `close()` completion callback fires, so `await using server = net.createServer()` works.
- **DNS lookup during `connect(host, port)`.** Resolving `host` to an address (when it isn't already a literal IP) reuses the same OS-resolution primitive `node:dns`'s `lookup()` needs — see 5.7; this is a genuine shared dependency between `node:net` and `node:dns`, not a hoist-for-convenience.

### 5.4 Multithread / worker interaction

- **`Server`/`Socket` handles are per-thread-region resources**, not naturally shareable: the underlying tokio task (accept loop / read loop) and the raw OS socket/pipe fd it owns live on whichever thread's tokio-runtime context spawned them. Per the RTS threading model (`docs/specs/rts-threading-model.md`), these map to `threadLocal`-owned handles by default — a `Server`/`Socket` `Handle` value is not something RTS should silently "promote to shared" the way plain data gets promoted on publication; passing a live connection to another RTS thread/worker requires an explicit transfer primitive (mirroring Node's `child_process`/`cluster` fd-passing, which is itself a distinct, more advanced feature — out of scope for this module's first implementation phases; see §7).
- **`BlockList`/`SocketAddress` are plain immutable value data** (rule list / address+family+port+flowlabel) — safe to serialize and reconstruct across threads/regions freely, consistent with the threading model's promotion-on-publication story for read-mostly immutable data. A `BlockList` handle crossing a channel should be treated as "clone the rule list, construct a fresh native `BlockList` in the target region" rather than literally sharing the Rust-side handle across threads.
- **Module-level config** (`net.getDefaultAutoSelectFamily`/`setDefaultAutoSelectFamily`, the attempt-timeout pair) is process-wide in real Node (a single global, not per-thread) — RTS should match that as a single shared `Mutex`/`RwLock`-guarded config cell reachable from any thread (unlike `node:dns`'s server list, which Node itself does *not* share across `worker_threads`; `node:net`'s two config globals are simple booleans/numbers with no per-thread-isolation precedent documented, so default to shared-not-thread-local here — flag as `(verify)` against real multi-worker Node behavior before locking this in).
- **Accept-loop and read-loop tasks run on the shared multi-thread tokio runtime**, same as every other async `rts-node`/`rts-std` feature — each such task must register with `gc/thread_registry`'s `on_thread_start`/`on_thread_stop` hooks (see 5.7) so the GC's conservative stack scanner sees any live handles (e.g. a `Socket` handle boxed into a `PolyValue` sitting on that task's Rust-side stack frame while it awaits) held across an `.await` point.

### 5.5 Buffer / TypedArray interop

This module's primary byte-data surface is the `'data'` event and `socket.write(buffer)` — unlike `node:dns` (one rare base64 field), byte throughput here is the common case, so it must **not** go through JSON/base64. `Buffer` (which `extends Uint8Array`, primordial engine-owned memory per the doctrine) crosses the ABI as a raw `(U64 ptr, U64 len)` pair pointing directly at engine-managed memory:

- `socket.write(buf)` → `.ts` shim extracts `buf.buffer`/`byteOffset`/`byteLength` and calls `__RTS_FN_NODE_NET_SOCKET_WRITE_BYTES(handle, ptr, len)` — the native write path reads directly from that memory region (no intermediate copy beyond what the OS `write(2)`/`WriteFile` call itself does).
- `'data'` event delivery: the native read loop allocates a fresh engine-owned buffer (through the same primordial `ArrayBuffer`/`Uint8Array` allocation path user code uses) sized to the bytes actually read, copies the OS-read bytes into it once, and invokes the `'data'` callback pointer with that buffer's handle — never a `StrPtr`/JSON round-trip.
- **String path** (`write(str, encoding)`, `'data'` after `setEncoding(enc)`) still crosses as `StrPtr` with the encoding name passed alongside; the actual `utf8`/`ascii`/`latin1`/`hex`/`base64`/`utf16le` conversion happens Rust-side using the shared `rts-node` encoding helper noted in 5.1 (not reinvented here).
- `SocketAddress`/`BlockList` carry no bulk byte data (only strings/small integers) — no TypedArray interop needed for those two classes.

### 5.6 Doctrine placement

`node:net` is **non-primordial** — the engine (`rts-codegen-new`) must never hardcode `"net"` or any `Server`/`Socket`/`BlockList`/`SocketAddress` member name. Resolution is purely data-driven, identical in shape to every other `node:` module: `import ... from 'node:net'` maps through `rts_node::ns_prefix_for("node:net")` → `"node_net"` (a lookup against `NODE_SPECS`, no hardcoded arm in codegen), and each call like `node_net.isIP(...)` resolves via `rts_node::node_lookup("node_net.isIP")` to a `NodespaceMember` (`symbol`, `args`, `returns`) — the same `NodespaceSpec`/`NODE_SPECS`/`node_lookup`/`ns_prefix_for` mechanism already implemented in `crates/rts-node/src/lib.rs` (currently populated by `fs`/`path`/`os`/`process`/`util`/`crypto`; `net` is a new sibling `NodespaceSpec`, not a special case).

Native-extern / `.ts`-shim split: every symbol in 5.2 is a raw primitive (handle lifecycle, byte/JSON in-out, callback-pointer registration, sync getters/setters). All JS-shape ergonomics live in a `.ts` shim shipped by `rts-node` (e.g. `rts-node/src/net/net.ts` + `server.ts` + `socket.ts` + `block_list.ts` + `socket_address.ts`):

- The `Server`/`Socket` classes, their `EventEmitter`-shaped `.on(name, cb)`/`.once`/`.emit` surface, and the "auto-registered `connectionListener`"/"one-time `connectListener`" sugar around `createServer`/`connect`.
- Overload resolution for the four `server.listen(...)` shapes and three `socket.connect(...)`/`createConnection(...)` shapes (inspecting arg count/type in `.ts`, not in native code).
- Option-object normalization (`ttl`-style boolean branches, `hints` bitmask assembly reusing `node:dns`'s `ADDRCONFIG`/`V4MAPPED`/`ALL` constants, ipv6-bracket `[addr]:port` string formatting for `SocketAddress`).
- JSON decoding of `AddressInfo`/`DropArgument`/`SocketAddress`/`BlockList.toJSON()` payloads into properly-shaped JS objects.
- `bufferSize`'s deprecated-alias-to-`writableLength` computed-property shim.

### 5.7 Shared-infra dependencies (FLAG)

- **Promise/async settle subsystem.** Needed only for `server[Symbol.asyncDispose]()` in this module's happy path (every other async surface here is callback/event-driven, not Promise-driven) — but every other async `node:*` module needs it too (per `docs/specs/async-promise-function.md`), so this remains a hard cross-cutting prerequisite currently living in `rts-std`'s `promise` namespace; must be hoisted into `rts-engine` or a new shared low crate before `rts-node` can implement `Symbol.asyncDispose` without depending on `rts-std`.
- **Shared tokio runtime (`rt()` in `rts-runtime/src/runtime/async_rt.rs`).** The accept loop, read loop, write-flush task, and the Happy-Eyeballs racer are all tokio-native and currently assume the single global multi-thread runtime that lives under `rts-std`/`rts-runtime`. This accessor needs to be reachable from `rts-node` without a `rts-std` dependency — same hoist needed by every other async-heavy node module (`node:dns`, `node:fs/promises`, etc.); should be solved once, not per-module.
- **GC thread-registry hooks (`on_thread_start`/`on_thread_stop`).** Every tokio task this module spawns (accept/read/write/racer) must register so the GC's conservative stack scanner sees handles (e.g. boxed `Socket`/`Buffer` `PolyValue`s) alive across `.await` points; this hook currently lives alongside the shared tokio setup in `rts-std`/`rts-runtime` and needs the same hoist.
- **HandleTable.** `Server`/`Socket`/`BlockList` handles need a `HandleTable`-shaped slab (shard-aware, gen16+slot48, per §2 of `02-runtime.md`). Prefer direct access to `rts-engine`'s `HandleTable` (the lowest layer, primordial-adjacent) rather than `rts-node` reimplementing its own shard logic from scratch — confirm `rts-engine::HandleTable` is importable from `rts-node` without pulling in `rts-std`.
- **OS-hostname-resolution primitive shared with `node:dns`.** `socket.connect(host, port)` (when `host` isn't a literal IP) and the Happy-Eyeballs racer both need exactly the same `getaddrinfo`-based `lookup(hostname, {family, all})` primitive that `node:dns`'s `dns.lookup()` implements. This should **not** be duplicated in two places — either `node:net`'s `.ts`/native layer calls into `node:dns`'s exported native lookup symbol directly (an intentional `node:net` → `node:dns` dependency, both being `rts-node` modules so this is allowed), or the primitive is factored into a small internal `rts-node` helper (e.g. `rts-node/src/_internal/hostlookup.rs`) that both `node:dns` and `node:net` call into. **Needs an explicit ordering/ownership decision before implementation phase (e)** — flagged in 5.8 and §7.
- **No TLS/crypto dependency** — `node:net` itself is plaintext TCP/IPC only (`node:tls` layers on top separately).
- **`EventEmitter` base for `Server`/`Socket` (`extends EventEmitter`).** Node's `EventEmitter` is itself non-primordial and (per `01-architecture.md`) the engine's own ambient `events` primitive lives on the `rts-shared`/`rts-runtime` side — not reachable from `rts-node`. This module must **not** reach for that; instead `Server`/`Socket` should extend either (a) a minimal `EventEmitter`-shaped base class implemented locally inside `rts-node`'s own `.ts` sources (a few dozen lines: `on`/`once`/`off`/`emit`/`listenerCount`), or (b) a future `node:events` module's exported class, if/when `rts-node` implements `node:events` — since both would be `rts-node`-owned, either choice keeps this module independent of `rts-std`. Recommend (a) first (unblocks `node:net` immediately) with a follow-up to swap in (b) once `node:events` exists, so `Server`/`Socket` end up `instanceof` the real `node:events`-exported `EventEmitter` for full parity.
- **Socket-level raw fn-pointer event callbacks (5.2/5.3) are *not* a shared-infra dependency** — they rely only on the engine's existing first-class-function-pointer materialization (already engine-level, not `rts-std`/`rts-shared`), deliberately avoiding the `Entry::Function`/`invoke_n` machinery so this module doesn't need to wait on that subsystem being made reachable.

### 5.8 Implementation phases

1. **(a)** Add `rts-node/src/net/mod.rs` with the `NodespaceSpec` skeleton (`node_module: "net"`, `ns_prefix: "node_net"`); register in `NODE_SPECS`. Implement `isIP`/`isIPv4`/`isIPv6` first (zero infra, pure string classification) to get the module wired end-to-end trivially.
2. **(b)** Implement `SocketAddress` (constructor + `parse` + read-only properties) and `BlockList` (`addAddress`/`addRange`/`addSubnet`/`check`/`toJSON`/`fromJSON`/`isBlockList`) — both sync, no async infra, establishing the handle-lifecycle pattern (`_NEW`/`_FREE`) reused by `Server`/`Socket`.
3. **(c)** Resolve the 5.7 blockers: hoist (or confirm reachability of) the promise/settle subsystem, shared tokio runtime accessor, GC thread-registry hooks, and `rts-engine::HandleTable` reachability. Decide the OS-hostname-lookup ownership question (shared helper vs. `node:net`-calls-`node:dns`) — this and the local `EventEmitter` base (5.7) are prerequisites for everything past this point.
4. **(d)** Implement the local `EventEmitter`-shaped `.ts` base class used by both `Server` and `Socket`.
5. **(e)** Implement single-family TCP `Socket`: `new Socket(options)`, `connect(port, host)` (no `autoSelectFamily` yet), `write`/`end`/`destroy`, `'connect'`/`'ready'`/`'data'`/`'end'`/`'close'`/`'error'`/`'drain'` events, `address()`/`bytesRead`/`bytesWritten`/`readyState`/`connecting`/`destroyed`/`pending`. Establishes the accept/read/write-task + raw-fn-pointer-callback pattern reused by everything else.
6. **(f)** Add IPv4/IPv6 dual-stack handling + `autoSelectFamily` Happy-Eyeballs racer + `'connectionAttempt'`/`'connectionAttemptFailed'`/`'connectionAttemptTimeout'` events + `autoSelectFamilyAttemptedAddresses` + `net.get/setDefaultAutoSelectFamily(AttemptTimeout)`.
7. **(g)** Implement TCP `Server`: `createServer`/`listen(port, host, backlog)`/`'connection'`/`'listening'`/`'close'`/`'error'`/`address()`/`ref`/`unref`/`maxConnections`/`'drop'`/`dropMaxConnection`; wire `net.connect`/`net.createConnection` port/host overload as thin wrappers over the `Socket` path from (e).
8. **(h)** Add IPC (Unix domain socket + Linux abstract-namespace) support for both `Socket.connect(path)` and `Server.listen(path)`, including `readableAll`/`writableAll` permission bits and the `Server.close()`-unlinks-the-path behavior.
9. **(i)** Add Windows named-pipe IPC backing (`tokio::net::windows::named_pipe`) behind `cfg(windows)`, matching the same `Server`/`Socket` JS surface as (h); confirm path-prefix detection (`\\?\pipe\`/`\\.\pipe\`) routes correctly.
10. **(j)** Remaining `Socket` surface: `setKeepAlive`/`setNoDelay`/`setTimeout`/`resetAndDestroy`/`getTypeOfService`/`setTypeOfService`/`pause`/`resume`/`setEncoding`/deprecated `bufferSize` alias/`onread` custom-buffer option/`fd`-wrapping constructor path.
11. **(k)** Remaining `Server` surface: `listen(handle)` overload (wrap an externally-provided fd/`Server`/`Socket`), `listen({signal})` `AbortSignal` wiring, `Symbol.asyncDispose`, `getConnections(callback)`.
12. **(l)** Wire `BlockList` into both `Server` (`blockList` option rejects inbound connections before `'connection'` fires) and `Socket` (`blockList` option rejects outbound `connect()` attempts before dialing).

## 6. Test plan

```
tests/node/net/net_is_ip.test.ts
  - net.isIP('127.0.0.1') === 4
  - net.isIP('::1') === 6
  - net.isIP('not-an-ip') === 0
  - net.isIPv4('192.168.1.1') === true; net.isIPv4('::1') === false
  - net.isIPv6('::1') === true; net.isIPv6('192.168.1.1') === false

tests/node/net/net_socket_address.test.ts
  - new net.SocketAddress() defaults to 127.0.0.1/ipv4
  - new net.SocketAddress({ family: 'ipv6' }) defaults address to '::'
  - net.SocketAddress.parse('123.1.2.3:1234') yields address/port/family
  - net.SocketAddress.parse('[1::1]:1234') yields ipv6 address/port
  - net.SocketAddress.parse('garbage') === undefined (no throw)
  - properties (address/family/port/flowlabel) are read-only

tests/node/net/net_block_list.test.ts
  - blockList.addAddress('1.2.3.4'); check('1.2.3.4') === true; check('1.2.3.5') === false
  - blockList.addRange('1.2.3.0', '1.2.3.255'); check('1.2.3.128') === true
  - blockList.addSubnet('10.0.0.0', 8); check('10.1.2.3') === true; check('11.0.0.1') === false
  - blockList.addAddress('::1', 'ipv6'); check('::1', 'ipv6') === true
  - net.BlockList.isBlockList(blockList) === true; isBlockList({}) === false
  - blockList.rules reflects insertion order and count
  - blockList.toJSON() round-trips through a fresh BlockList().fromJSON(json)
  - addSubnet with out-of-range prefix (e.g. 200 for ipv4) throws

tests/node/net/net_server_tcp_basic.test.ts
  - net.createServer().listen(0, '127.0.0.1', () => { ... }) binds an ephemeral port; server.address().port > 0
  - 'connection' fires with a Socket when a client connects; server.getConnections(cb) reflects count
  - server.close(cb) fires 'close' and cb with no error; address() returns null after close
  - listen() a second time without closing throws ERR_SERVER_ALREADY_LISTEN
  - server.ref()/unref() do not throw and do not affect functional behavior of the test process

tests/node/net/net_server_error_addrinuse.test.ts
  - two servers listening on the same fixed port: second emits 'error' with code EADDRINUSE, no automatic 'close'
  - EADDRINUSE retry pattern (close + relisten after timeout) eventually succeeds once the port frees

tests/node/net/net_socket_client_basic.test.ts
  - net.connect(port, 'localhost', cb) / net.createConnection(port) fires 'connect' then 'ready'
  - socket.write('hello') returns boolean; server-side 'data' receives matching bytes
  - socket.end() triggers server-side 'end'; with default allowHalfOpen server socket then auto-closes
  - socket.destroy(new Error('boom')) fires 'error' immediately followed by 'close' with hadError === true
  - socket.setEncoding('utf8') makes 'data' deliver strings instead of Buffer
  - socket.pause()/resume() stop/resume 'data' delivery observably

tests/node/net/net_socket_write_backpressure.test.ts
  - writing a payload larger than the OS socket buffer while the reader is paused returns false and 'drain' fires once the reader resumes and drains it
  - deprecated socket.bufferSize tracks writable.writableLength while backlogged

tests/node/net/net_allow_half_open.test.ts
  - default (allowHalfOpen: false): after remote FIN ('end'), the socket auto-ends its writable side
  - allowHalfOpen: true: after remote FIN, writable side stays open until explicit end() is called

tests/node/net/net_ipv6_dualstack.test.ts
  - server.listen({ port: 0 }) with no host binds '::' and accepts both an IPv4-mapped and a native IPv6 client (where the OS supports it)
  - server.listen({ port: 0, host: '::', ipv6Only: true }) rejects/does not accept an IPv4 connection attempt

tests/node/net/net_autoselectfamily.test.ts
  - connecting to a dual-stack hostname with autoSelectFamily: true populates socket.autoSelectFamilyAttemptedAddresses
  - 'connectionAttempt'/'connectionAttemptFailed'/'connectionAttemptTimeout' fire with correct (ip, port, family) shape
  - all-attempts-fail case surfaces a single AggregateError on 'error' wrapping every per-attempt error
  - net.setDefaultAutoSelectFamily(false); a new socket without an explicit option does not run the racer
  - net.setDefaultAutoSelectFamilyAttemptTimeout(5) is clamped up to 10 internally (observable via attempt timing)

tests/node/net/net_max_connections_drop.test.ts
  - server.maxConnections = 1; a second concurrent connection triggers 'drop' with a well-formed DropArgument (TCP)
  - server.maxConnections = 0 drops every incoming connection (v21+ semantics, not Infinity)

tests/node/net/net_ipc_unix_socket.test.ts (skip on Windows)
  - server.listen('/tmp/rts-test.sock'); client net.connect('/tmp/rts-test.sock') connects and exchanges data
  - server.close() unlinks the socket path from the filesystem
  - server.listen('\0rts-abstract-test') (Linux abstract socket) is not visible via fs.existsSync and still connectable
  - a path exceeding sun_path length throws synchronously

tests/node/net/net_ipc_named_pipe.test.ts (Windows only)
  - server.listen('\\\\.\\pipe\\rts-test'); client net.connect('\\\\.\\pipe\\rts-test') connects and exchanges data
  - pipe does not appear in any filesystem listing and disappears once both ends close

tests/node/net/net_block_list_integration.test.ts
  - net.createServer({ blockList }) with blockList.addAddress(clientIp) rejects that client's inbound connection before 'connection' fires
  - net.connect({ port, host, blockList }) with the target address blocked refuses to dial (rejects/errors before any 'connectionAttempt')

tests/node/net/net_type_of_service.test.ts
  - socket.setTypeOfService(0x10); socket.getTypeOfService() === 0x10
  - new net.Socket({ typeOfService: 0x08 }).getTypeOfService() === 0x08 once connected
  - setTypeOfService(256) throws ERR_OUT_OF_RANGE

tests/node/net/net_async_dispose.test.ts
  - `await using server = net.createServer(); server.listen(0);` auto-closes the server at scope exit (Symbol.asyncDispose)

tests/node/net/net_worker_threads.test.ts (multithread)
  - a Server/Socket handle created on the main thread is not silently usable from a spawned worker_thread (either rejected structured-clone-style, or an explicit transfer API is required) — assert the isolation, not literal sharing
  - net.setDefaultAutoSelectFamily(false) on main thread: assert observed behavior on a worker thread (verify shared-vs-isolated per 5.4's flagged uncertainty)
  - many concurrent TCP servers/sockets across N worker threads operate independently without cross-interference (stress: M connections per worker, assert per-worker byte-count integrity)
```

## 7. Open questions / deferrals

- **OS-hostname-lookup primitive ownership (5.7).** Whether `node:net`'s `socket.connect(host, port)`/Happy-Eyeballs racer calls into `node:dns`'s native `lookup` symbol directly, or both modules share a smaller internal `rts-node` helper, is an explicit open design call — needs resolving before implementation phase (f)/(e) rather than duplicating the `getaddrinfo` wrapper in two places.
- **Module-level config thread-scoping (5.4).** Whether `net.getDefaultAutoSelectFamily`/`setDefaultAutoSelectFamily`/attempt-timeout pair should be a single process-wide value (as assumed here) or per-thread-isolated (as `node:dns`'s server config is) is flagged `(verify)` — real Node's behavior across `worker_threads` for these two specific globals was not directly confirmed from the fetched docs and should be checked empirically before locking in the RTS threading-model mapping.
- **`fd`-passing / `cluster`-style socket handoff.** Node's `server.listen({fd})`/passing a live socket to a child process (`child_process.fork()` + `server.listen(handle)`) is explicitly out of scope for this spec's implementation phases (5.8) — deferred until an RTS `node:cluster`/`node:child_process` spec exists to define the cross-process handle-transfer contract.
- **`reusePort` on unsupported platforms.** Exact error shape (Node just "raises an error" per its own docs, without a documented specific error code) needs picking a concrete `Error`/code for RTS — likely `ERR_INVALID_ARG_VALUE`-style, marked `(verify)` against real Node's thrown error on an unsupported platform.
- **Abstract Unix sockets on non-Linux POSIX (macOS/BSD).** Node's abstract-socket support is Linux-specific; RTS should simply reject (not silently reinterpret) a leading-`\0` path on non-Linux Unix, matching Node.
- **Windows named-pipe backlog/queue-depth semantics** vs POSIX `backlog`/`somaxconn` are not directly analogous; exact RTS behavior for the `backlog` option when routed through the named-pipe backend is deferred to implementation-time experimentation (phase i), not specified precisely here.
- **`BlockList`/`SocketAddress` crossing worker channels** (structured-clone semantics) — Node does not define a native transferability story for these either (they're plain-ish objects); RTS's choice to "serialize rules/fields and reconstruct" (5.4) is an implementation choice, not a strict Node-parity requirement, and open to revision.
- **Exact Happy-Eyeballs concurrency model.** This spec assumes Node's real implementation is sequential-with-per-attempt-timeout (matching the "each connection attempt... is given the amount of time... before timing out and trying the next address" wording) rather than a fully concurrent fan-out; if empirical testing against real Node reveals a subtler concurrent/staggered-start behavior, phase (f) should be revisited to match it exactly rather than the simplified sequential racer described in 5.1.

## 8. What actually landed (2026-07-16)

Phases (a) and (b) of §5.8, complete: `crates/rts-node/src/net/` — `ip.rs`
(classifiers), `blocklist/{mod,rules}.rs`, `socket_address.rs`, `mod.rs`
(registration). Where the implementation chose differently from §5, this section
is the truth.

### Done, and real

- **`BlockList`** — every member (`addAddress`/`addRange`/`addSubnet`/`check`/
  `toJSON`/`fromJSON`/`rules`/`isBlockList`), with Node's ACTUAL matching
  semantics read off `src/node_sockaddr.cc` rather than guessed:
  rules match **across families** (an IPv4 rule covers the IPv4-mapped IPv6 form
  `::ffff:1.2.3.4`, and vice versa; a non-mapped IPv6 never matches an IPv4
  rule), a range's cross-family compare is *unordered* (so neither `>=` nor `<=`
  holds → no match), and the CIDR masks follow Node's own `in_network_*`
  bit math including the non-byte-aligned IPv6 prefix case. The rule strings are
  Node's (see §2) — `toJSON()`/`fromJSON()` round-trip through them.
- **`SocketAddress`** — constructor (+ the family-dependent address default),
  `parse()` (returns nothing for garbage, never throws), and the four read-only
  properties. Immutable by construction: the instance carries its fields and the
  class registers getters and no setters.
- **Wired into `node:dgram`**: `createSocket({ receiveBlockList, sendBlockList })`
  now takes a real `BlockList` (a snapshot of its rules, read once at creation,
  as Node does). A blocked SENDER's datagram is dropped in the reader thread
  before any `'message'` listener sees it; a blocked DESTINATION is refused with
  an `ERR_IP_BLOCKED` error delivered to the send callback / `'error'` listener
  instead of being dialed. Verified end-to-end via `rts run` (a blocked receiver
  gets nothing while a control receiver with an empty list gets its datagram).

### Divergences from the plan

- **No `.ts` shim** (§5.2/§5.6 assumed one): `rts-node` ships no `.ts`. Both
  classes are object-backed Registry classes, the model `node:fs`'s `Stats` and
  `node:dgram`'s `Socket` use.
- **No JSON at the ABI** (§5.2 proposed `StrPtr` JSON for options/addresses):
  members take `AbiType::PolyValue` and branch on the value's tag, so a
  `SocketAddress` passed where Node accepts `string | SocketAddress` is read
  through its own class data with no string round trip.
- **`BlockList` needs no `_FREE`**: its rules live in a side table keyed by the
  instance handle, and `SocketAddress` needs no table at all (its four fields
  ride the instance).

### `Server` + `Socket` — DONE (TCP), same day

`crates/rts-node/src/net/tcp/` (`state`/`opts`/`server`/`socket`/`props`/`pump`).
Real TCP: `listen` binds through `socket2` (SO_REUSEADDR like Node/libuv, an
explicit `listen(backlog)` — default **511**, not 512 — `IPV6_V6ONLY`,
`SO_REUSEPORT`), an ACCEPT THREAD queues each connection, a READ THREAD per
connection queues bytes, and `pump.rs` builds the JS values on the JS thread.
Verified end to end via `rts run`: listen → `'connection'` (with a real
`remoteAddress`) → `'connect'`/`readyState` → data both ways → `bytesRead`/
`bytesWritten` → `'close'` with `hadError=false` → server `'close'`.

Implemented: `createServer`/`connect`/`createConnection` (every overload,
resolved BY VALUE), `Server` (`listen` ×4 overloads, `close`, `address`,
`getConnections`, `ref`/`unref`, `listening`, `'connection'`/`'listening'`/
`'close'`/`'error'`/`'drop'`), `Socket` (`new Socket(options)`, `connect` ×3,
`write` ×3, `end` ×4, `destroy`/`destroySoon`/`resetAndDestroy`, `pause`/
`resume`, `setEncoding`, `setNoDelay`/`setKeepAlive`/`setTimeout`,
`getTypeOfService`/`setTypeOfService`, `address`, and every property:
`remoteAddress`/`remoteFamily`/`remotePort`/`localAddress`/`localFamily`/
`localPort`/`bytesRead`/`bytesWritten`/`bufferSize`/`connecting`/`pending`/
`destroyed`/`readyState`/`timeout`/`autoSelectFamilyAttemptedAddresses`), the
`autoSelectFamily` **Happy-Eyeballs racer** (AAAA-first-then-A, per-attempt
timeout, `'connectionAttempt'`/`'connectionAttemptFailed'`) and its process-wide
config pair. Node's asymmetry is preserved: a `Server`'s `'error'` does NOT
auto-close it; a `Socket`'s `'error'` is always followed by `'close'`.

Refused, not ignored (`ERR_INVALID_ARG_VALUE`): `path` (IPC — Unix-domain
sockets / Windows named pipes), `fd`, `onread`, `signal`, `lookup`,
`readableAll`/`writableAll`.

The stream-inherited surface (`pipe`/`read`/`highWaterMark`/`writableLength`,
async iteration) is NOT here — `net.Socket` is a `stream.Duplex` in Node and
`node:stream` does not exist yet. Everything §2 documents as `Socket`'s OWN
surface is implemented; the Duplex inheritance is what waits on `node:stream`.

### Engine gaps this work found (generic, fixed or recorded)

Three were FIXED generically (they benefit every object-backed Registry class,
not just net):

1. **Untracked-receiver method shapes.** `try_runtime_ci` (the marshaller behind
   `server.on('connection', s => s.on('data', …))`, where `s` is a param with no
   static class) enumerated a fixed set of ABI shapes and silently missed
   `(this, StrPtr, PolyValue)` — the whole EventEmitter surface. Added that and
   the other shapes the backend classes use. An F64 **argument** is still absent
   on purpose: on Win64 a double rides an XMM register, so an all-integer
   transmute would hand the callee garbage — unsupported beats silently wrong.
2. **Getters were never harvested** into the runtime CI table (methods only), so
   a COMPUTED property on a callback's receiver (`socket.remoteAddress`, which
   reads the OS) always read `undefined` — `Stats` had hidden this by STORING
   its fields in the instance map. Getters are harvested now and `obj_get`
   consults the table when a map has no stored field by that name.
3. **`dgram.Socket` vs `net.Socket` collided.** The Registry is keyed by class
   name globally and `insert_class` REPLACES, so registering `net`'s `Socket`
   silently wiped `dgram`'s. Node genuinely has two `Socket` classes; since
   `dgram.Socket` is not constructible (only `createSocket()` yields one), its
   registry key does not have to match a user-written `new X()` — it is now
   `DgramSocket`. (A dotted `dgram.Socket` does NOT work: the ts-signature's
   return-class parser reads the dot as a type boundary.) Cosmetic divergence:
   `constructor.name` on a dgram socket.

One is RECORDED, not worked around:

4. **A property WRITE on an object-backed class instance does not land**
   (`server.maxConnections = 1` leaves the property `undefined`; `obj_set` does
   not write the instance map, and `MemberKind::InstanceSetter` is dead metadata
   — codegen never calls `Class::instance_setter`). So `maxConnections`/
   `dropMaxConnection` — plain properties in Node, which the accept thread reads
   off the object — cannot be set yet, which means the `maxConnections` limit and
   its `'drop'` event are NOT reachable from JS today. The accept-thread side is
   implemented and correct; it starts working the moment the engine writes the
   field. Not asserted as working in any test.
