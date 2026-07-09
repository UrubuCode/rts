# node:dgram

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:dgram` |
| Node.js version | 25.x |
| Stability | 2 - Stable (long-standing core API since Node 0.1.x; the current doc page prints no explicit stability box — verify wording against the live doc, but treat as Stable) |
| Tier | P1 |
| Status | [ ] Not implemented — spec only |
| Import forms | `import dgram from "node:dgram"`; `import { createSocket, Socket } from "node:dgram"`; CJS `require("node:dgram")` / legacy bare `require("dgram")` |
| Globals exposed | None — `node:dgram` does not add anything to `globalThis` |

## 1. Purpose

`node:dgram` implements UDP (User Datagram Protocol) datagram sockets: connectionless,
unordered, unreliable message delivery over IPv4/IPv6. It exposes a single
`Socket` class (an `EventEmitter`) created via the factory function
`dgram.createSocket()` — there is no public constructor. The module covers
socket lifecycle (bind/connect/close), sending/receiving datagrams, multicast
group membership (including source-specific multicast), and low-level socket
tuning (TTL, broadcast, send/recv buffer sizes). It has **no Promise-based
API** — every operation is either synchronous (throws) or callback/event
based, unlike many other newer Node modules.

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `class Socket extends EventEmitter`

Not directly constructible — `new dgram.Socket()` is not part of the public
API. The only way to obtain an instance is `dgram.createSocket(...)`.

**Events** — see [2.4](#24-events) for full detail; summarized here as they
belong to `Socket`: `'close'`, `'connect'`, `'error'`, `'listening'`,
`'message'`.

**Instance methods** (26 total):

##### Lifecycle

| Method | Signature |
|---|---|
| `bind` (overload A) | `bind(port?: number, address?: string, callback?: () => void): void` |
| `bind` (overload B) | `bind(options: BindOptions, callback?: () => void): void` |
| `close` | `close(callback?: () => void): void` |
| `connect` | `connect(port: number, address?: string, callback?: (err?: Error) => void): void` |
| `disconnect` | `disconnect(): void` |
| `[Symbol.asyncDispose]` | `[Symbol.asyncDispose](): Promise<void>` |

**`bind(port?, address?, callback?)` / `bind(options, callback?)`**

| Param | Type | Optional | Default |
|---|---|---|---|
| `port` | `number` | yes | `0` (OS picks a random port) |
| `address` | `string` | yes | all interfaces (`'0.0.0.0'` udp4 / `'::'` udp6) |
| `options` | `BindOptions` | — | — |
| `callback` | `() => void` | yes | — (also fires as `'listening'`) |

Returns: `void`. Throws: none typically — errors are asynchronous, delivered
via `'error'`. Variant: **callback + event** (asynchronous even though the
underlying `bind(2)`/`bind()` syscall is fast — Node's contract guarantees at
least one event-loop tick of delay).

**`close(callback?)`**

| Param | Type | Optional | Default |
|---|---|---|---|
| `callback` | `() => void` | yes | — (also fires as `'listening'` → `'close'` listener) |

Returns: `void`. Throws: none documented for the common case (closing an
already-closed socket is effectively a no-op in current Node; older versions
may raise `ERR_SOCKET_DGRAM_NOT_RUNNING` — verify). Variant: **callback +
event**.

**`connect(port, address?, callback?)`**

| Param | Type | Optional | Default |
|---|---|---|---|
| `port` | `number` | no | — |
| `address` | `string` | yes | `'127.0.0.1'` (udp4) / `'::1'` (udp6) |
| `callback` | `(err?: Error) => void` | yes | — |

Returns: `void`. Throws: `ERR_SOCKET_DGRAM_IS_CONNECTED` if already connected.
Variant: **callback + event** (errors passed to callback if provided,
otherwise emitted as `'error'`).

**`disconnect()`**

No params. Returns: `void`. Throws: `ERR_SOCKET_DGRAM_NOT_CONNECTED` if the
socket is unbound or not connected. Variant: **sync**.

**`[Symbol.asyncDispose]()`**

No params. Returns: `Promise<void>`, resolves once the socket is closed
(internally wraps `close()`). Variant: **promise**. Added v20.5.0/v18.18.0,
stable since v24.2.0.

##### Addressing

| Method | Signature |
|---|---|
| `address` | `address(): AddressInfo` |
| `remoteAddress` | `remoteAddress(): AddressInfo` |

**`address()`** — no params. Returns `AddressInfo`. Throws `EBADF` if the
socket is unbound. Variant: **sync**.

**`remoteAddress()`** — no params. Returns `AddressInfo` of the connected
peer. Throws `ERR_SOCKET_DGRAM_NOT_CONNECTED` if the socket is not connected.
Variant: **sync**.

##### Sending

| Method | Signature |
|---|---|
| `send` (no offset/length) | `send(msg: MsgLike, callback?: SendCallback): void` |
| `send` (with port/address) | `send(msg: MsgLike, port?: number, address?: string, callback?: SendCallback): void` |
| `send` (with offset/length, Buffer-like only) | `send(msg: Buffer \| TypedArray \| DataView, offset: number, length: number, port?: number, address?: string, callback?: SendCallback): void` |

`MsgLike = Buffer | TypedArray | DataView | string | Array<Buffer | TypedArray | DataView | string>`.

| Param | Type | Optional | Default |
|---|---|---|---|
| `msg` | `MsgLike` | no | — |
| `offset` | `number` | yes (Buffer/TypedArray/DataView only — invalid with string/Array) | — |
| `length` | `number` | yes (same restriction) | — |
| `port` | `number` | yes if socket is connected or already bound with a fixed peer | — |
| `address` | `string` | yes | `'127.0.0.1'` (udp4) / `'::1'` (udp6) |
| `callback` | `SendCallback` | yes | — |

Returns: `void`. Throws: `ERR_SOCKET_BAD_PORT` if called on an unbound socket
without a resolvable port. Auto-binds to a random port + all-interfaces
address if the socket was never bound. Variant: **callback** (the callback
is "the only reliable way to know when a datagram was sent"; DNS errors go
to the callback if present, else emitted as `'error'`).

##### Multicast membership

| Method | Signature |
|---|---|
| `addMembership` | `addMembership(multicastAddress: string, multicastInterface?: string): void` |
| `addSourceSpecificMembership` | `addSourceSpecificMembership(sourceAddress: string, groupAddress: string, multicastInterface?: string): void` |
| `dropMembership` | `dropMembership(multicastAddress: string, multicastInterface?: string): void` |
| `dropSourceSpecificMembership` | `dropSourceSpecificMembership(sourceAddress: string, groupAddress: string, multicastInterface?: string): void` |

All four: Returns `void`. Auto-binds to a random port if unbound. Throws
`EADDRINUSE` if `addMembership`/`addSourceSpecificMembership` is called more
than once for the same group on the same worker under `node:cluster`.
Variant: **sync**.

##### Socket tuning (TTL, broadcast, multicast interface)

| Method | Signature |
|---|---|
| `setTTL` | `setTTL(ttl: number): void` |
| `setMulticastTTL` | `setMulticastTTL(ttl: number): void` |
| `setMulticastLoopback` | `setMulticastLoopback(flag: boolean): void` |
| `setBroadcast` | `setBroadcast(flag: boolean): void` |
| `setMulticastInterface` | `setMulticastInterface(multicastInterface: string): void` |

| Method | Param | Type | Range | Default | Throws |
|---|---|---|---|---|---|
| `setTTL` | `ttl` | `number` | 1–255 | 64 | `EBADF` if unbound |
| `setMulticastTTL` | `ttl` | `number` | 0–255 | 1 | `EBADF` if unbound |
| `setMulticastLoopback` | `flag` | `boolean` | — | — | `EBADF` if unbound |
| `setBroadcast` | `flag` | `boolean` | — | — | `EBADF` if unbound |
| `setMulticastInterface` | `multicastInterface` | `string` | platform-dependent format (§4) | — | `EBADF` unbound; `EINVAL`/`EADDRNOTAVAIL`/`EPROTONOSUP` on bad address; "Not running" if socket closed |

All: Returns `void`. Variant: **sync**.

##### Buffer sizing

| Method | Signature |
|---|---|
| `setSendBufferSize` | `setSendBufferSize(size: number): void` |
| `setRecvBufferSize` | `setRecvBufferSize(size: number): void` |
| `getSendBufferSize` | `getSendBufferSize(): number` |
| `getRecvBufferSize` | `getRecvBufferSize(): number` |
| `getSendQueueSize` | `getSendQueueSize(): number` |
| `getSendQueueCount` | `getSendQueueCount(): number` |

All: Throws `ERR_SOCKET_BUFFER_SIZE` if called on an unbound socket (except
`getSendQueueSize`/`getSendQueueCount`, which report the libuv internal write
queue and do not require a bound socket — they simply report 0/empty).
`getSendQueueSize`/`getSendQueueCount` added v18.8.0/v16.19.0. Variant:
**sync**.

##### Reference counting

| Method | Signature |
|---|---|
| `ref` | `ref(): Socket` |
| `unref` | `unref(): Socket` |

Both: no params, return the socket itself (chainable). `ref()` restores
default behavior (an open/bound socket keeps the process alive); `unref()`
excludes the socket from the process's reference count so the process may
exit even while the socket is open. Calling either multiple times has no
additional effect. Variant: **sync**.

### 2.2 Top-level functions

| Function | Signature |
|---|---|
| `createSocket` (options form) | `createSocket(options: SocketOptions, callback?: MessageListener): Socket` |
| `createSocket` (type form) | `createSocket(type: SocketType, callback?: MessageListener): Socket` |

**`createSocket(options, callback?)`** — added v0.11.13.

| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `SocketOptions` | no | — |
| `callback` | `MessageListener` | yes | — (attached as a `'message'` listener) |

Returns: `Socket`. Throws: none synchronously for valid `type`; invalid
`type` throws a `TypeError`/`ERR_INVALID_ARG_VALUE`-class error. Variant:
**sync** (the returned socket itself does async work later).

**`createSocket(type, callback?)`** — added v0.1.99.

| Param | Type | Optional | Default |
|---|---|---|---|
| `type` | `SocketType` (`'udp4'` \| `'udp6'`) | no | — |
| `callback` | `MessageListener` | yes | — |

Returns: `Socket`. Same throw/variant notes as above.

This is the **only** top-level function the module exports (`functionCount`
= 1 for this spec's bookkeeping — the two signatures are overloads of the
same exported name).

### 2.3 Properties & constants

None. Unlike `fs.constants`, `os.constants`, or `crypto.constants`,
`node:dgram` exports no module-level constants or properties.

### 2.4 Events

All events are emitted on a `Socket` instance.

| Event | Listener signature | Fires when |
|---|---|---|
| `'close'` | `() => void` | After `close()` completes and the underlying socket handle is fully released. |
| `'connect'` | `() => void` | After a successful `connect()` call. |
| `'error'` | `(exception: Error) => void` | On any socket error not delivered to a more specific callback (unhandled ⇒ process crash, see §4). |
| `'listening'` | `() => void` | Once the socket is bound and addressable/ready to receive data. Fires once after `bind()`, or implicitly before the first `send()` on an unbound socket. |
| `'message'` | `(msg: Buffer, rinfo: MessageInfo) => void` | On every inbound datagram. `msg` is the payload; `rinfo` carries sender address/family/port/size. |

## 3. Types & option objects

```typescript
type SocketType = "udp4" | "udp6";

interface SocketOptions {
  type: SocketType;                 // required
  reuseAddr?: boolean;              // default: false — SO_REUSEADDR
  reusePort?: boolean;              // default: false — SO_REUSEPORT (v23.1.0+; Linux 3.9+/DragonFlyBSD 3.6+/FreeBSD 12.0+/Solaris 11.4/AIX 7.2.5+ only)
  ipv6Only?: boolean;               // default: false — IPV6_V6ONLY
  recvBufferSize?: number;          // SO_RCVBUF, applied at bind time
  sendBufferSize?: number;          // SO_SNDBUF, applied at bind time
  lookup?: (
    hostname: string,
    options: { family: 0 | 4 | 6; all?: boolean },
    callback: (err: Error | null, address: string, family: number) => void,
  ) => void;                        // default: dns.lookup()
  signal?: AbortSignal;             // v15.8.0+ — abort() closes the socket
  receiveBlockList?: NetBlockList;  // filters inbound datagrams by sender address
  sendBlockList?: NetBlockList;     // filters outbound datagrams by destination address
}

interface BindOptions {
  port?: number;
  address?: string;
  exclusive?: boolean;   // default: false — see cluster note in §4
  fd?: number;            // bind to an already-open fd instead of a fresh socket
}

interface AddressInfo {
  address: string;   // e.g. "127.0.0.1" or "::1"
  family: string;     // "IPv4" | "IPv6"
  port: number;
}

interface MessageInfo extends AddressInfo {
  size: number;   // datagram payload size in bytes
}

type MessageListener = (msg: Buffer, rinfo: MessageInfo) => void;
type SocketReadyCallback = () => void;              // bind()/close()
type SocketErrorCallback = (err?: Error) => void;   // connect()
type SendCallback = (error: Error | null) => void;  // send()

// Forward-referenced, not part of node:dgram itself:
type NetBlockList = import("node:net").BlockList;
```

Note: `remoteAddress()` and `address()` both return the 3-field
`AddressInfo` shape (no `size`); only the `'message'` event's `rinfo`
carries the 4-field `MessageInfo` shape.

## 4. Node semantics & edge cases

### Datagram size limits

Max IPv4/IPv6 datagram size is governed by the 16-bit Payload Length field
(65,507 bytes usable = 65,535 − 8-byte UDP header − 20-byte IP header) but
is realistically bounded by path MTU: IPv4 minimum 68 octets, recommended
576 for dial-up-class links, 1500 for modern Ethernet; IPv6 minimum 1280,
with a mandatory 1500-byte reassembly buffer. **There is no MTU discovery.
Sending a datagram larger than the receiver's MTU silently drops it — no
error is returned to the sender.** This is a fundamental UDP property, not
a Node/RTS quirk, and must not be "fixed" by inventing a fake error.

### `bind()` behavior

- `port` omitted or `0` ⇒ OS assigns a random ephemeral port.
- `address` omitted ⇒ OS listens on all interfaces.
- A bound socket keeps the Node process alive until closed (see `ref`/`unref`).
- Binding is asynchronous by contract — `'listening'` fires after the call
  returns, never synchronously inline.
- **Cluster + `EADDRINUSE`**: `socket.addMembership()` (and
  `addSourceSpecificMembership()`) must be called **at most once per
  cluster worker process** for a given group; multiple workers joining the
  same multicast group on the same port raises `EADDRINUSE`.
- **`exclusive`**: default `false` ⇒ cluster workers share the underlying
  socket handle (connection handling shared across workers). `true` ⇒ the
  handle is NOT shared; port-sharing attempts error. When `reusePort: true`
  is set on `createSocket`, `exclusive` is implicitly always `true` on
  `bind()`.

### Unbound-socket throws (all via `EBADF` unless noted)

`address()`, `setBroadcast()`, `setTTL()`, `setMulticastLoopback()`,
`setMulticastInterface()`, `setMulticastTTL()` all throw `EBADF` on an
unbound socket. `getRecvBufferSize()`/`getSendBufferSize()`/
`setRecvBufferSize()`/`setSendBufferSize()` throw `ERR_SOCKET_BUFFER_SIZE`
instead (Node-specific error class, not a raw errno).

### `send()` / DNS behavior

- Default address if omitted: `'127.0.0.1'` (udp4) / `'::1'` (udp6).
- Implicit bind: an unbound socket auto-binds to a random port on the
  all-interfaces address before the first send.
- Hostname resolution delays the send by **at least one event-loop tick**;
  resolution errors go to the callback if provided, else emitted as
  `'error'`.
- `msg` accepts `Buffer`/`TypedArray`/`DataView`/`string`/`Array<...>`;
  strings are UTF-8-encoded automatically. `offset`/`length` are valid ONLY
  for `Buffer`/`TypedArray`/`DataView` (byte-oriented forms) — invalid to
  combine with `string`/`Array`. For multi-byte-character strings,
  `offset`/`length` (when they would apply) are computed in **bytes**, not
  character/code-point positions.
- Calling `send()` on an unbound socket with no resolvable port throws
  `ERR_SOCKET_BAD_PORT`.

### `connect()` behavior

- Throws `ERR_SOCKET_DGRAM_IS_CONNECTED` if already connected.
- Once connected: `send()`'s port/address params are no longer accepted (or
  are redundant/ignored per the connected-socket send overload); the socket
  only receives datagrams from the connected peer; unsolicited datagrams
  from other senders are dropped at the OS level (`ECONNREFUSED`-style ICMP
  handling is platform-dependent).

### IPv6 multicast interface — platform-specific scope syntax

- **POSIX/Linux/macOS**: interface **name** as scope, e.g. `'::%eth1'`.
- **Windows**: interface **number** as scope, e.g. `'::%2'`.
- **IPv4** (all platforms): the interface's own IP address, e.g. `'10.0.0.2'`.
- Restore system default: `'0.0.0.0'` (IPv4) / `'::'` (IPv6).
- Errors: `EINVAL` if the string cannot be parsed; `EADDRNOTAVAIL` /
  `EPROTONOSUP` for IPv4 family mismatches; most IPv6 scope
  errors silently fall back to the system default instead of throwing.

### Error events

- **An unhandled `'error'` listener crashes the process** — exactly like
  any other `EventEmitter` in Node, `dgram.Socket` has no default `'error'`
  handler. Production code must always attach one.
- DNS errors during `send()`: callback if present, else `'error'` event.
- `bind()` errors: emitted as `'error'` (rare cases may throw synchronously
  for clearly invalid arguments before any syscall happens).
- `connect()` errors: callback if present, else `'error'` event.

### Error code reference table

| Code | Raised by | Meaning |
|---|---|---|
| `ERR_SOCKET_DGRAM_IS_CONNECTED` | `connect()` | Socket already connected. |
| `ERR_SOCKET_DGRAM_NOT_CONNECTED` | `disconnect()`, `remoteAddress()` | Socket is unbound or not connected. |
| `ERR_SOCKET_BAD_PORT` | `send()` | Unbound socket, no resolvable port. |
| `ERR_SOCKET_BUFFER_SIZE` | `get/setSendBufferSize`, `get/setRecvBufferSize` | Buffer-size op on unbound socket. |
| `EBADF` | `address()`, `setBroadcast()`, `setTTL()`, `setMulticastLoopback()`, `setMulticastInterface()`, `setMulticastTTL()` | Op on unbound socket. |
| `EINVAL` | `setMulticastInterface()` | Malformed interface string. |
| `EADDRNOTAVAIL` / `EPROTONOSUP` | `setMulticastInterface()` | IPv4 address family mismatch. |
| `EADDRINUSE` | `addMembership()` (under cluster) | Group already joined by another worker. |

### Process-exit / `ref`/`unref`

A bound socket keeps the process alive by default. `unref()` excludes it
from that accounting (process may exit with the socket still open);
`ref()` restores default accounting. Both are idempotent and chainable.

### `AbortSignal` support (v15.8.0+)

`createSocket({ type, signal })` — aborting the controller closes the
socket (equivalent to calling `close()`).

### Block lists (v15.8.0+)

`receiveBlockList`/`sendBlockList` (both `net.BlockList` instances) filter
inbound/outbound datagrams by IP/range/subnet. **Does not work behind a
reverse proxy/NAT** — the checked address is the proxy's address, not the
original client's.

### Version-history notes worth knowing

- `'message'` event's `rinfo.family`: was a `string`, briefly became a
  `number` in v18.0.0, reverted back to `string` in v18.4.0. RTS should
  only ever implement the (current, stable) string form.
- `send()`: `Array` support + optional offset/length since v5.7.0; success
  callback's first arg changed from `0` to `null` in v6.0.0; `Uint8Array`
  support + always-optional `address` since v8.0.0; `TypedArray`/`DataView`
  since v14.5.0/v12.19.0; `address` restricted to `string | null |
  undefined` since v17.0.0.
- No methods in the current surface are deprecated.

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node` is fully independent — it does **not** depend on `rts-std` and
implements its own native backing using Rust `std` plus a small number of
its own crate dependencies (mirroring how it already plans to bring in
`flate2`/`rustls`/etc. for other modules).

| Surface area | Backing |
|---|---|
| Socket create/bind/connect/close, `send`/`recv`, `local_addr`/`peer_addr`, `set_broadcast`, unicast `set_ttl` | **`socket2` crate** (`socket2::Socket`), a strict superset of `std::net::UdpSocket` that additionally exposes buffer sizes and multicast options through one consistent type. Recommended as the SOLE socket representation (not `std::net::UdpSocket` directly) — it converts freely to/from `std::net::UdpSocket` (`.into()`) for any interop that needs the std type (e.g. handing to `tokio::net::UdpSocket::from_std`). |
| `join_multicast_v4`/`v6`, `leave_multicast_v4`/`v6`, `set_multicast_loop_v4`/`v6`, `set_multicast_ttl_v4` | `socket2::Socket` methods. **Gap**: `socket2` has no IPv6 multicast-hop-limit (`IPV6_MULTICAST_HOPS`) setter as of the versions surveyed — needs raw `setsockopt` (see below) for `setMulticastTTL()` on a `udp6` socket. Mark `(verify)` against the pinned `socket2` version at implementation time. |
| `getSendBufferSize`/`getRecvBufferSize`/`setSendBufferSize`/`setRecvBufferSize` | `socket2::Socket::{send_buffer_size, recv_buffer_size, set_send_buffer_size, set_recv_buffer_size}`. |
| `addSourceSpecificMembership`/`dropSourceSpecificMembership` (IGMPv3/MLDv2 source-specific multicast) | **Not covered by `std` or `socket2`.** Raw `setsockopt` FFI: Unix via the `libc` crate (`IP_ADD_SOURCE_MEMBERSHIP`/`IP_DROP_SOURCE_MEMBERSHIP` + `ip_mreq_source` for v4; `MCAST_JOIN_SOURCE_GROUP`/`MCAST_LEAVE_SOURCE_GROUP` + `group_source_req` for v6); Windows via the `windows-sys` crate (`Win32::Networking::WinSock`, equivalent `IP_ADD_SOURCE_MEMBERSHIP` struct from `ws2ipdef.h`). One of the two hardest corners of this module — see phase (i). |
| `setMulticastInterface` name↔index resolution (POSIX interface name vs Windows interface number scope) | `libc::if_nametoindex` (Unix) / `windows-sys`'s `Win32::NetworkManagement::IpHelper::if_nametoindex` (Windows, available since Vista) — same function name on both platforms, small cross-platform shim. |
| `getSendQueueSize`/`getSendQueueCount` | No OS-level equivalent (these mirror libuv's internal async-write queue). Approximate with a per-socket `AtomicUsize` byte counter and `AtomicUsize` request counter, incremented before an async write is dispatched and decremented on completion (see 5.3). For the synchronous send path (the common case) both read effectively 0 — an honest approximation, not a silent hardcode (flagged in §7). |
| `ref()`/`unref()` | No OS equivalent — pure RTS event-loop bookkeeping flag stored alongside the socket entry (see 5.3/5.7 gap). |
| Inbound datagram delivery (`'message'`) | A background reader (see 5.3) — Node's own model relies on libuv's always-running reactor, which RTS's current event loop does not have (see 5.7 flag #1). |

**Handle storage**: `rts-engine::heap::handles` already defines
`Entry::UdpSocket(Box<UdpEntry>)` with `UdpEntry { socket: std::net::UdpSocket,
last_peer: Option<SocketAddr> }` — this lives in the **engine** crate (not
`rts-std`), and `alloc_entry`/`with_entry`/`with_entry_mut`/`free_handle` are
public free functions there. `rts-node`'s dgram module can allocate/read/free
through this EXISTING engine-level `Entry` variant with **zero new
`rts-engine` dependency** and zero coupling to `rts-std`. Two paths forward,
to decide during implementation:

1. **Reuse `Entry::UdpSocket` as-is**, but its `UdpEntry.socket` field is
   typed `std::net::UdpSocket`, not `socket2::Socket` — either widen
   `UdpEntry` (extend the struct with the extra bookkeeping fields dgram
   needs: ref-count flag, pending-send atomics, an internal inbound-message
   queue handle, and swap the field type to `socket2::Socket`), or
2. Add the extra dgram-only bookkeeping as a **new, dgram-specific `Entry`
   variant** in `rts-engine` (e.g. `Entry::NodeUdpSocket(Box<NodeUdpEntry>)`)
   so the pre-existing `UdpEntry`/`Entry::UdpSocket` (historically wired for
   `rts-std`'s now-being-removed `net` namespace) is left alone/deleted
   cleanly rather than reshaped underneath two different owners mid-migration.

Recommendation: since the owner decision already removes `rts-std`'s
duplicated `net` module, option 1 (widen/rename `UdpEntry` in place once
`rts-std::net`'s UDP path is deleted) is the leaner outcome; treat option 2
as the safe interim if the removal and the dgram build land in different
PRs and must not collide.

### 5.2 ABI surface

`ns_prefix = "node_dgram"`, `node_module = "dgram"`, registered in
`rts-node`'s `NODE_SPECS` exactly like `fs`/`process`/`os`/`path`/`util`/
`crypto` today. All rich values (the socket itself) are opaque `u64`
Handles; the `Socket` class, event wiring, option normalization,
overload disambiguation, default-address selection, and error-code → `Error`
object mapping all live in a `.ts` shim — the externs below are the raw
primitive surface only.

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_DGRAM_CREATE` | `Bool(is_udp6), Bool(reuse_addr), Bool(reuse_port), Bool(ipv6_only)` | `Handle` | Allocates the OS socket (unbound). |
| `__RTS_FN_NODE_DGRAM_BIND` | `Handle, I32(port), StrPtr(address), Bool(exclusive)` | `I32` (0 = ok, else negated errno) | Synchronous at the ABI level; the `.ts` shim defers `'listening'`/callback firing to the microtask/macrotask queue to preserve Node's "always async" contract. |
| `__RTS_FN_NODE_DGRAM_CLOSE` | `Handle` | `Void` | Frees the handle; stops the background reader (5.3). |
| `__RTS_FN_NODE_DGRAM_CONNECT` | `Handle, I32(port), StrPtr(address)` | `I32` (status) | |
| `__RTS_FN_NODE_DGRAM_DISCONNECT` | `Handle` | `I32` (status; nonzero ⇒ not-connected) | |
| `__RTS_FN_NODE_DGRAM_ADDRESS_IP` | `Handle` | `StrPtr` | Empty/error sentinel if unbound. |
| `__RTS_FN_NODE_DGRAM_ADDRESS_PORT` | `Handle` | `I32` | |
| `__RTS_FN_NODE_DGRAM_ADDRESS_FAMILY` | `Handle` | `StrPtr` (`"IPv4"`/`"IPv6"`) | |
| `__RTS_FN_NODE_DGRAM_REMOTE_IP` / `_PORT` / `_FAMILY` | `Handle` | `StrPtr` / `I32` / `StrPtr` | Mirrors the address getters for the connected peer. |
| `__RTS_FN_NODE_DGRAM_SEND` | `Handle, U64(data_ptr), I64(data_len), I32(port), StrPtr(address), Bool(has_port)` | `I32` (status) | `data_ptr`/`data_len` come from the stable `ArrayBuffer`/`Buffer` pointer (5.5); strings/Arrays are pre-flattened into one contiguous buffer by the `.ts` shim before this call. |
| `__RTS_FN_NODE_DGRAM_SET_TTL` | `Handle, I32` | `I32` | |
| `__RTS_FN_NODE_DGRAM_SET_MULTICAST_TTL` | `Handle, I32` | `I32` | |
| `__RTS_FN_NODE_DGRAM_SET_MULTICAST_LOOPBACK` | `Handle, Bool` | `I32` | |
| `__RTS_FN_NODE_DGRAM_SET_BROADCAST` | `Handle, Bool` | `I32` | |
| `__RTS_FN_NODE_DGRAM_SET_MULTICAST_INTERFACE` | `Handle, StrPtr` | `I32` | |
| `__RTS_FN_NODE_DGRAM_ADD_MEMBERSHIP` | `Handle, StrPtr(group), StrPtr(iface), Bool(has_iface)` | `I32` | |
| `__RTS_FN_NODE_DGRAM_DROP_MEMBERSHIP` | `Handle, StrPtr(group), StrPtr(iface), Bool(has_iface)` | `I32` | |
| `__RTS_FN_NODE_DGRAM_ADD_SOURCE_MEMBERSHIP` | `Handle, StrPtr(source), StrPtr(group), StrPtr(iface), Bool(has_iface)` | `I32` | Phase (i) — raw setsockopt. |
| `__RTS_FN_NODE_DGRAM_DROP_SOURCE_MEMBERSHIP` | `Handle, StrPtr(source), StrPtr(group), StrPtr(iface), Bool(has_iface)` | `I32` | Phase (i). |
| `__RTS_FN_NODE_DGRAM_SET_SEND_BUFFER_SIZE` / `_RECV_` | `Handle, I64(size)` | `I32` | |
| `__RTS_FN_NODE_DGRAM_GET_SEND_BUFFER_SIZE` / `_RECV_` | `Handle` | `I64` | |
| `__RTS_FN_NODE_DGRAM_GET_SEND_QUEUE_SIZE` / `_COUNT` | `Handle` | `I64` | Approximated (5.1). |
| `__RTS_FN_NODE_DGRAM_REF` / `_UNREF` | `Handle` | `Void` | `.ts` shim returns `this` for chaining. |
| `__RTS_FN_NODE_DGRAM_POLL` | `Handle` | `I32` (0 = nothing ready, 1 = a message was dequeued) | Drains one datagram from the socket's inbound queue into "current message" slots; called once per event-loop tick per bound socket with a `'message'` listener. |
| `__RTS_FN_NODE_DGRAM_CURRENT_MSG_HANDLE` | `Handle` | `Handle` (a `Buffer`/`ArrayBuffer` handle) | Valid only immediately after a `POLL` that returned 1. |
| `__RTS_FN_NODE_DGRAM_CURRENT_MSG_ADDRESS` / `_FAMILY` | `Handle` | `StrPtr` | |
| `__RTS_FN_NODE_DGRAM_CURRENT_MSG_PORT` | `Handle` | `I32` | |
| `__RTS_FN_NODE_DGRAM_CURRENT_MSG_SIZE` | `Handle` | `I64` | |

`[Symbol.asyncDispose]()` needs **no new extern** — the `.ts` shim
implements it as `async [Symbol.asyncDispose]() { return new Promise<void>(r => this.close(() => r())); }`,
riding on the already-primordial `Promise` constructor.

### 5.3 Async model

- **`bind`/`connect`/`send` callbacks and `'listening'`/`'connect'`
  events**: the underlying syscalls are effectively synchronous/fast, but
  Node's contract guarantees at least one tick of delay. The `.ts` shim
  defers firing via the engine's microtask/macrotask primitives (needs the
  shared queue — flagged in 5.7).
- **Hostname resolution** (`address` args that are not literal IPs, and the
  `lookup` option): performed via `std::net::ToSocketAddrs`
  (blocking `getaddrinfo`), dispatched through the shared multi-thread tokio
  runtime's `spawn_blocking` so it never blocks the JS thread — mirrors the
  existing `thread.spawn_async_join` pattern. Needs the shared tokio runtime
  (flagged in 5.7).
- **Inbound `'message'` delivery**: `dgram` needs a genuinely async source
  of datagrams even though its own ABI is synchronous. Recommended v1
  design: **one dedicated OS thread per bound+listening socket**
  (`std::thread::spawn`, blocking `recv_from` loop) pushing `(Buffer bytes,
  sender addr)` into a per-socket `Mutex<VecDeque<...>>`; the JS-thread side
  calls `POLL` once per event-loop tick to drain it. This avoids a tokio
  dependency for the core recv path (UDP sockets are typically few per
  process) — a pooled `tokio::net::UdpSocket` reactor is a viable future
  upgrade if profiling shows thread-per-socket doesn't scale (e.g. many
  concurrently-bound sockets), not required for v1.
- **`send()`'s own dispatch**: the OS `sendto()` call is fast/non-blocking
  in the common case; run it directly on the calling JS thread and invoke
  the callback via the microtask queue on completion (not `spawn_blocking`)
  unless DNS resolution is also needed for this call (then piggy-back on
  the same `spawn_blocking` resolution step).
- **No Promise API surface**: `dgram` never returns a `Promise` from any of
  its documented methods except `[Symbol.asyncDispose]()`, which needs no
  native promise plumbing (5.2). The `PromiseSlot`/`promise.create` native
  subsystem is **not** required by this module.

### 5.4 Multithread / worker interaction

Per `docs/specs/rts-threading-model.md` (worker = RTS thread/region,
`MessagePort` = channel, `SharedArrayBuffer` = shared heap):

- A `dgram.Socket` wraps a raw OS resource (fd/HANDLE) and is **not**
  transferable or shareable across a `MessagePort`/worker boundary — this
  mirrors Node exactly (Node does not support transferring a dgram socket
  to a `Worker` either). Each RTS thread/region that wants a UDP socket
  creates and owns one independently; there is no cross-thread sharing of
  a single live socket handle by design.
- `rts-engine`'s `HandleTable` is already shard-aware and thread-safe, so a
  `Handle` value *could* technically be looked up from any OS thread — but
  relying on that to actually share one live socket between two
  worker-thread regions would violate Node's own semantics and invite races
  Node itself forbids. Treat "one socket, one owning thread/region" as a
  hard design constraint, not just an implementation detail.
- `exclusive`/`reusePort` (`SO_REUSEPORT`/`SO_REUSEADDR`) exist to let
  multiple **OS processes** (Node's `cluster` module — separate address
  spaces) bind the same port and let the OS load-balance. This is a
  multi-**process** pattern, distinct from RTS's in-process
  `threadLocal`/`shared` worker model; it maps to a future `node:cluster`
  spec, not to `worker_threads`.
- The background reader thread per socket (5.3) is itself an RTS-internal
  thread, invisible to the JS-level threading model — it must be joined/
  stopped on `close()` to avoid a leaked thread outliving its socket.

### 5.5 Buffer / TypedArray interop

`msg` (send) and the delivered `'message'` payload cross the ABI as a raw
`(ptr: u64, len: i64)` pair backed by the primordial `ArrayBuffer`/`Buffer`
memory model (`Buffer extends Uint8Array`; the stable byte pointer is the
same one the N-API `ArrayBuffer` machinery already exposes via
`Entry::ArrayBuffer` in `rts-engine`). No `StrPtr`/GC-string-pool
involvement — datagram payloads are raw bytes, not necessarily valid UTF-8,
so they must never round-trip through the string pool. `offset`/`length`
for the `Buffer`/`TypedArray`/`DataView` `send()` overloads select a
sub-range of that pointer (`ptr + offset`, `length`), bounds-checked
natively against the buffer's total `byteLength` before the `sendto` call.
String `msg` arguments are first UTF-8-encoded into a fresh `Buffer` by the
`.ts` shim (the same encode step `TextEncoder` already performs), so the
native `SEND` extern only ever handles the one `(ptr, len)` shape.
`Array<...>` (scatter) sends are flattened into one contiguous buffer by
the shim for v1 (§7 notes this forgoes true vectored `sendmsg`/`WSASend`).
Inbound datagrams are copied from the OS receive buffer into a freshly
allocated `Buffer` handle sized to the **actual** bytes received (which may
be smaller than the pre-allocated read buffer), so `msg.length ===
rinfo.size` holds structurally.

### 5.6 Doctrine placement

`node:dgram` is unambiguously **non-primordial** — it has no native literal
syntax; a `UdpSocket` is reached only via `dgram.createSocket()`, an
ordinary function call. The engine must never hardcode the string
`"dgram"` (or `"Socket"`) anywhere in `crates/rts-codegen-new/`. Resolution
path: `import ... from "node:dgram"` → `ns_prefix_for("node:dgram")` →
`"node_dgram"` → `node_lookup("node_dgram.<member>")` → the matching
`NodespaceMember` in `rts-node`'s `dgram::SPEC` (mirrors exactly how
`node:fs`/`node:process` resolve today via `NODE_SPECS`). This is the
"registry for node:" — a **data table**, not a codegen `match` arm; adding
`dgram` support means adding a new `NodespaceSpec` entry, never touching
engine control flow. The JS-facing ergonomics (the `Socket` class,
`EventEmitter`-style `.on`/`.emit`, overload disambiguation, default
addresses, error-code → `Error` mapping) live entirely in a `.ts` shim
shipped by `rts-node`; only the raw primitive ops in §5.2 are native
`extern "C"` symbols.

Because `Socket extends EventEmitter` and `rts-node` cannot depend on
`rts-std` (where the existing `EventEmitter`/`RtsEventsEmitter` primitives
live), the `.ts` shim should implement its **own** minimal internal
event-emitter base (an array of `{event, listener}` pairs + manual
`emit`/`on`/`once`/`removeListener`, calling listeners via the primordial
`Function` class) rather than reaching for either of `rts-std`'s existing
emitter implementations. See §7 for the cross-module consolidation
question once a second `EventEmitter`-based node module is speced.

### 5.7 Shared-infra dependencies (FLAG)

- **Event-loop "keep alive while a handle is active" semantics.** RTS's
  current event loop (`rts-std::event_loop::run_event_loop`) is a single
  bounded drain — microtasks → immediates → macrotasks → pending timers →
  pending promises → done — not a persistent reactor. A long-lived
  bound+listening UDP socket needs the process to stay alive and keep
  polling for inbound datagrams until `.close()`/until its ref-count drops
  to zero, which this drain model does not provide today. This is
  cross-cutting infra `rts-node` cannot build unilaterally (it is the
  process's single top-level driver) — must be resolved at the shared/
  engine level.
- **Shared multi-thread tokio runtime** (`rts-std::runtime::async_rt::rt()`).
  Needed for non-blocking DNS hostname resolution via `spawn_blocking`
  (5.3), and optionally for a future pooled-reactor recv path. Currently
  lives in `rts-std`, unreachable from `rts-node` without violating the
  no-`rts-std`-dependency rule.
- **Microtask/macrotask queue primitives** (`rts-std::globals::timers`).
  Needed to give `bind`/`connect`/`send` callbacks Node's "always
  asynchronous, never same-tick" contract.
- **`AbortController`/`AbortSignal`** (`rts-std::globals::abort`). Needed
  for the `signal` `createSocket()` option. Currently implemented as a web
  global inside `rts-std`.
- **`net.BlockList`.** Needed for `receiveBlockList`/`sendBlockList`.
  **Does not exist anywhere in the codebase yet** — not even in `rts-std`.
  This is new infra, not a hoist; since a future `node:net` will need the
  identical primitive, building it once (wherever it ends up living) rather
  than duplicating it per-module is worth flagging now.
- **Promise subsystem** (`rts-std::promise` / `PromiseSlot`) — **not
  needed**. `dgram`'s public surface is 100% callback/`EventEmitter` based;
  the sole exception, `[Symbol.asyncDispose]()`, is satisfiable purely via
  the already-primordial `Promise` constructor from the `.ts` shim (5.2),
  so no flag applies here.

### 5.8 Implementation phases

a. **Core lifecycle** — `CREATE`/`BIND`/`CLOSE`/`CONNECT`/`DISCONNECT`/
   `ADDRESS_*`/`REMOTE_*` externs over `socket2::Socket`; udp4 only; no
   multicast, no buffer sizing, no ref/unref, no inbound messages yet.
   Enough for a bind-then-close fixture.
b. **Outbound send** to a literal (non-hostname) address — wire the
   Buffer/TypedArray/DataView/string `msg` forms through the ArrayBuffer
   pointer bridge (5.5); connected-socket `send()` variant too.
c. **Inbound message path** — background reader thread + inbound queue +
   `POLL`/`CURRENT_MSG_*` externs + `.ts` shim wiring into the event loop
   (best-effort against the not-yet-extended drain model, see §7);
   `'message'`/`'listening'`/`'error'`/`'close'` events.
d. **udp6 support** — address-family auto-detection for every op that
   branches on v4 vs v6 (multicast, TTL, interface scope format).
e. **Multicast (ASM)** — `addMembership`/`dropMembership` (v4 + v6) via
   `socket2`'s join/leave; `setMulticastTTL`/`setMulticastLoopback`/
   `setBroadcast`/`setMulticastInterface` including the
   `if_nametoindex`-based cross-platform scope resolution.
f. **Buffer sizing** — `get/setSendBufferSize`, `get/setRecvBufferSize` via
   `socket2`; `getSendQueueSize`/`getSendQueueCount` via the atomic
   approximation (5.1).
g. **`ref()`/`unref()`** — wired into the extended event-loop keep-alive
   bookkeeping; depends on the 5.7 event-loop gap landing first.
h. **DNS + advanced `createSocket` options** — hostname resolution via
   `spawn_blocking` on the shared tokio runtime, custom `lookup` option,
   `signal` (`AbortSignal`) support, `exclusive`/`reusePort` bind-option
   plumbing (cluster orchestration itself deferred, see §7).
i. **Source-specific multicast** — `addSourceSpecificMembership`/
   `dropSourceSpecificMembership` via raw `setsockopt` FFI (`libc` on Unix,
   `windows-sys` on Windows) — neither `std` nor `socket2` cover IGMPv3/
   MLDv2 SSM.
j. **Block lists** — `receiveBlockList`/`sendBlockList`, once `net.BlockList`
   exists (5.7).

## 6. Test plan

1. **Basic bind + address()** (udp4): `bind(0)`, assert `address().port > 0`
   and `address().family === 'IPv4'`.
2. **Loopback send/receive round trip**: two sockets on `127.0.0.1`; sender
   `send()`s to the receiver's bound port; receiver's `'message'` fires
   with the correct payload bytes and `rinfo.address`/`rinfo.port`/
   `rinfo.size`.
3. **`connect()` then `send()` without port/address**: verify
   `remoteAddress()` matches the connected peer and that `send()` reaches
   it using only the connected-socket overload.
4. **Double `connect()`**: second call throws `ERR_SOCKET_DGRAM_IS_CONNECTED`.
5. **`disconnect()` while not connected**: throws
   `ERR_SOCKET_DGRAM_NOT_CONNECTED`.
6. **`send()` on unbound socket**: without a port throws
   `ERR_SOCKET_BAD_PORT`; with a port, auto-binds and succeeds.
7. **`close()` lifecycle**: emits `'close'`, fires the callback, and a
   background-reader thread (if any) is joined/stopped; further sends
   after close fail cleanly (no crash).
8. **`'error'` event on bind conflict**: binding two sockets to the same
   fixed port without `reuseAddr` produces an `'error'` (not a process
   crash) when a listener is attached.
9. **`setBroadcast(true)` + broadcast send** (`255.255.255.255`):
   best-effort; mark skip-if-unsupported in CI sandboxing.
10. **Multicast join/leave**: `addMembership('239.1.2.3')` on a udp4 socket
    bound to the group's port; a second socket sends to the group; assert
    delivery; `dropMembership` stops further delivery.
11. **TTL/multicast-TTL/buffer-size round trips**: set then get returns a
    plausible value (allow OS-side rounding/clamping in assertions).
12. **`ref()`/`unref()` chaining**: `socket.ref() === socket` and
    `socket.unref() === socket`.
13. **udp6 loopback** (`::1`): bind + send + message round trip; `family`
    reported as `'IPv6'`.
14. **Multi-byte UTF-8 string send**: receiver's `Buffer` bytes match
    `Buffer.from(str, 'utf8')`; `rinfo.size` equals the **byte** length,
    not the character length.
15. **`Array` (scatter) send**: an array of buffers reassembles correctly
    on the receiving end.
16. **Multithread isolation**: create sockets on N separate RTS worker
    threads (per the threading model), each bound to its own random port;
    assert no cross-talk and no shared-handle corruption (each thread's
    `Socket` is fully independent, per §5.4).
17. **`createSocket({ signal })`**: abort the controller before and after
    `bind()`; assert the socket closes (`'close'` fires) and no further
    `'message'` events are delivered.
18. **Oversized-datagram smoke test** (document-only / skip on CI): sending
    an implausibly large payload must not crash the process, whether it
    errors synchronously (`EMSGSIZE`) or is silently dropped.

## 7. Open questions / deferrals

- **Event-loop keep-alive semantics** (5.7 #1) is a prerequisite for a
  fully faithful long-lived listening socket. Until it lands, dgram
  fixtures may need an explicit bounded wait in the `.ts` test harness as
  an interim workaround — a known, stated gap, not a silent limitation.
- **Source-specific multicast (SSM)** and **IPv6 multicast-hop-limit**
  setting have no coverage in `std` or `socket2`; raw `setsockopt` via
  `libc`/`windows-sys` is proposed but unverified end-to-end on every
  target platform (Windows SSM constants especially) — `(verify)`.
- **`net.BlockList`** does not exist anywhere in the codebase.
  `receiveBlockList`/`sendBlockList` are deferred until it (or an
  `rts-node`-local equivalent) is built.
- **`AbortController`/`AbortSignal`** currently lives only in `rts-std`
  (`globals/abort`) as a web global. Whether it gets hoisted to a shared
  low crate, reimplemented independently inside `rts-node`, or `dgram`
  simply rejects/ignores the `signal` option until resolved is an open
  call for the owner.
- **`getSendQueueSize()`/`getSendQueueCount()`** are inherently
  libuv-internal concepts with no OS-level source of truth; the proposed
  atomic-counter approximation is close for the common (synchronous OS
  send) case but will read differently from real Node under heavy async
  backpressure — an acceptable, explicitly-flagged divergence rather than
  a silent hardcoded `0`.
- **Scatter/gather `Array` sends** are implemented as shim-side
  concatenation (one copy) rather than true vectored `sendmsg`/`WSASend` —
  correct but not zero-copy; revisit if profiling calls for it.
- **`EventEmitter` base for `Socket`**: bespoke `.ts`-only implementation
  (recommended here, zero native surface) vs. reusing a hoisted-shared
  emitter primitive if/when a second `EventEmitter`-based node module
  (`net.Server`, `fs.watch`, …) is speced — worth deciding once that second
  consumer exists, to avoid three independent reimplementations.
- **Cluster-mode `exclusive`/`reusePort`** semantics depend on
  `node:cluster` (multi-process fork model), out of scope here; this spec
  plumbs the OS-level `SO_REUSEPORT`/`SO_REUSEADDR` flags but defers the
  multi-process orchestration itself to a future `node:cluster` spec.
