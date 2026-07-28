# node:dns

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:dns` (+ `node:dns/promises`) |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P1 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import dns from 'node:dns'`; `import dnsPromises from 'node:dns/promises'`; `import { Resolver } from 'node:dns'`; `const dns = require('node:dns')`; `const dnsPromises = require('node:dns/promises')` |
| Globals exposed | none (all access is via the `node:dns` / `node:dns/promises` module imports; no ambient globals) |

## 1. Purpose

`node:dns` provides hostname-to-address resolution and the reverse (address-to-hostname), plus raw DNS resource-record queries (MX, TXT, SRV, SOA, CAA, NAPTR, NS, PTR, CNAME, TLSA, ANY). It exposes two resolution families with materially different semantics: `dns.lookup()`/`dns.lookupService()` go through OS name resolution facilities (`getaddrinfo`/`getnameinfo`, `/etc/hosts`, `nsswitch.conf`), while `dns.resolve()` and friends always speak the DNS protocol directly against configured servers, bypassing OS-level hosts files. The module also exposes the `dns.Resolver` class for creating independently-configured resolvers (custom servers/timeouts) that do not affect global DNS configuration, and a promise-based mirror (`dns.promises` / `node:dns/promises`) of the entire surface.

## 2. Exported API surface (COMPLETE)

### Classes

#### `dns.Resolver`

Not a subclass of `EventEmitter` (no events). Independent DNS resolver instance with its own server list, timeout, and tries configuration — does not touch global `dns` module state.

```typescript
class Resolver {
  constructor(options?: ResolverOptions);

  resolve(hostname: string, callback: (err: NodeJS.ErrnoException | null, records: string[] | AnyRecord[]) => void): void;
  resolve(hostname: string, rrtype: string, callback: (err: NodeJS.ErrnoException | null, records: string[] | AnyRecord[]) => void): void;

  resolve4(hostname: string, callback: (err: NodeJS.ErrnoException | null, addresses: string[]) => void): void;
  resolve4(hostname: string, options: { ttl: true }, callback: (err: NodeJS.ErrnoException | null, addresses: Array<{ address: string; ttl: number }>) => void): void;
  resolve4(hostname: string, options: { ttl: false }, callback: (err: NodeJS.ErrnoException | null, addresses: string[]) => void): void;

  resolve6(hostname: string, callback: (err: NodeJS.ErrnoException | null, addresses: string[]) => void): void;
  resolve6(hostname: string, options: { ttl: true }, callback: (err: NodeJS.ErrnoException | null, addresses: Array<{ address: string; ttl: number }>) => void): void;
  resolve6(hostname: string, options: { ttl: false }, callback: (err: NodeJS.ErrnoException | null, addresses: string[]) => void): void;

  resolveAny(hostname: string, callback: (err: NodeJS.ErrnoException | null, records: AnyRecord[]) => void): void;
  resolveCaa(hostname: string, callback: (err: NodeJS.ErrnoException | null, records: CaaRecord[]) => void): void;
  resolveCname(hostname: string, callback: (err: NodeJS.ErrnoException | null, addresses: string[]) => void): void;
  resolveMx(hostname: string, callback: (err: NodeJS.ErrnoException | null, addresses: MxRecord[]) => void): void;
  resolveNaptr(hostname: string, callback: (err: NodeJS.ErrnoException | null, addresses: NaptrRecord[]) => void): void;
  resolveNs(hostname: string, callback: (err: NodeJS.ErrnoException | null, addresses: string[]) => void): void;
  resolvePtr(hostname: string, callback: (err: NodeJS.ErrnoException | null, addresses: string[]) => void): void;
  resolveSoa(hostname: string, callback: (err: NodeJS.ErrnoException | null, address: SoaRecord) => void): void;
  resolveSrv(hostname: string, callback: (err: NodeJS.ErrnoException | null, addresses: SrvRecord[]) => void): void;
  resolveTlsa(hostname: string, callback: (err: NodeJS.ErrnoException | null, records: TlsaRecord[]) => void): void;
  resolveTxt(hostname: string, callback: (err: NodeJS.ErrnoException | null, records: string[][]) => void): void;

  reverse(ip: string, callback: (err: NodeJS.ErrnoException | null, hostnames: string[]) => void): void;

  getServers(): string[];
  setServers(servers: readonly string[]): void;
  setLocalAddress(ipv4?: string, ipv6?: string): void;
  cancel(): void;
}
```

Base class: none (plain `class Resolver`, no EventEmitter). Events: none.

#### `dns.promises.Resolver` (aka `require('node:dns/promises').Resolver`)

Identical shape to `dns.Resolver` except every `resolve*`/`reverse` method returns a `Promise` instead of taking a callback; `getServers`, `setServers`, `setLocalAddress`, `cancel` are unchanged (sync, void/array return). Base class: none. Events: none.

### Top-level functions (`node:dns`, callback style)

| Function | Variant |
|---|---|
| `dns.lookup(hostname[, options], callback)` | callback |
| `dns.lookupService(address, port, callback)` | callback |
| `dns.resolve(hostname[, rrtype], callback)` | callback |
| `dns.resolve4(hostname[, options], callback)` | callback |
| `dns.resolve6(hostname[, options], callback)` | callback |
| `dns.resolveAny(hostname, callback)` | callback |
| `dns.resolveCaa(hostname, callback)` | callback |
| `dns.resolveCname(hostname, callback)` | callback |
| `dns.resolveMx(hostname, callback)` | callback |
| `dns.resolveNaptr(hostname, callback)` | callback |
| `dns.resolveNs(hostname, callback)` | callback |
| `dns.resolvePtr(hostname, callback)` | callback |
| `dns.resolveSoa(hostname, callback)` | callback |
| `dns.resolveSrv(hostname, callback)` | callback |
| `dns.resolveTlsa(hostname, callback)` | callback |
| `dns.resolveTxt(hostname, callback)` | callback |
| `dns.reverse(ip, callback)` | callback |
| `dns.getServers()` | sync |
| `dns.setServers(servers)` | sync |
| `dns.setDefaultResultOrder(order)` | sync |
| `dns.getDefaultResultOrder()` | sync |
| `dns.setLocalAddress([ipv4][, ipv6])` | sync |

#### `dns.lookup(hostname[, options], callback)`

Resolves a hostname (e.g. `'example.org'`) into the first found A (IPv4) or AAAA (IPv6) record, using the OS's `getaddrinfo(3)` (libuv threadpool).

Params:

| Name | Type | Optional | Default |
|---|---|---|---|
| `hostname` | `string` | no | — |
| `options` | `number \| LookupOptions` | yes | `{ family: 0, hints: 0, all: false, verbatim: true, order: 'verbatim' }` |
| `callback` | `(err: NodeJS.ErrnoException \| null, address: string, family: number) => void` (or `(err, addresses: LookupAddress[]) => void` when `all: true`) | yes (required in practice; last-arg overload) | — |

Return: `void`. Throws: none synchronously beyond `TypeError` for bad argument types (`ERR_INVALID_ARG_TYPE`); resolution failures go to `callback(err)` with `err.code` set (`ENOTFOUND`, etc). Variant: callback.

#### `dns.lookupService(address, port, callback)`

Resolves an `(address, port)` pair into a hostname and service name via `getnameinfo(3)`.

| Name | Type | Optional | Default |
|---|---|---|---|
| `address` | `string` | no | — |
| `port` | `number` | no | — |
| `callback` | `(err: NodeJS.ErrnoException \| null, hostname: string, service: string) => void` | no | — |

Return: `void`. Throws: `ERR_INVALID_ARG_TYPE` on bad types. Variant: callback.

#### `dns.resolve(hostname[, rrtype], callback)`

Generic resource-record resolver; dispatches to one of `resolve4`/`resolve6`/`resolveAny`/… based on `rrtype`.

| Name | Type | Optional | Default |
|---|---|---|---|
| `hostname` | `string` | no | — |
| `rrtype` | `'A'\|'AAAA'\|'ANY'\|'CAA'\|'CNAME'\|'MX'\|'NAPTR'\|'NS'\|'PTR'\|'SOA'\|'SRV'\|'TLSA'\|'TXT'` | yes | `'A'` |
| `callback` | `(err: NodeJS.ErrnoException \| null, records: string[] \| object[] \| object) => void` | no | — |

Return: `void`. Throws (via callback err.code): `ENOTFOUND`, `ESERVFAIL`, `ETIMEOUT`, and any `dns.<CODE>` constant below prefixed `E`. Variant: callback.

#### `dns.resolve4(hostname[, options], callback)`

| Name | Type | Optional | Default |
|---|---|---|---|
| `hostname` | `string` | no | — |
| `options` | `{ ttl?: boolean }` | yes | `{ ttl: false }` |
| `callback` | `(err, addresses: string[] \| Array<{address:string; ttl:number}>) => void` | no | — |

Return: `void`. Variant: callback.

#### `dns.resolve6(hostname[, options], callback)`

Same shape as `resolve4` for AAAA records.

#### `dns.resolveAny(hostname, callback)`

| Name | Type | Optional |
|---|---|---|
| `hostname` | `string` | no |
| `callback` | `(err, ret: AnyRecord[]) => void` | no |

Each element of `ret` has a `type` discriminant (`'A'|'AAAA'|'CNAME'|'MX'|'NAPTR'|'NS'|'PTR'|'SOA'|'SRV'|'TXT'`) plus type-specific fields. Note: `ANY` is not a real DNS record type; results are not guaranteed complete (some servers omit types like `AAAA` even when present) — Node docs explicitly warn against relying on `resolveAny()`. Variant: callback.

#### `dns.resolveCaa(hostname, callback)`

Returns `CaaRecord[]` (`critical`, `issue?`, `issuewild?`, `iodef?`, `contactemail?`, `contactphone?`). Variant: callback.

#### `dns.resolveCname(hostname, callback)` → `string[]`. Variant: callback.

#### `dns.resolveMx(hostname, callback)` → `MxRecord[]`. Variant: callback.

#### `dns.resolveNaptr(hostname, callback)` → `NaptrRecord[]`. Variant: callback.

#### `dns.resolveNs(hostname, callback)` → `string[]` (name-server hostnames). Variant: callback.

#### `dns.resolvePtr(hostname, callback)` → `string[]`. Variant: callback.

#### `dns.resolveSoa(hostname, callback)` → `SoaRecord` (single object, not array). Variant: callback.

#### `dns.resolveSrv(hostname, callback)` → `SrvRecord[]`. Variant: callback.

#### `dns.resolveTlsa(hostname, callback)` → `TlsaRecord[]`. Variant: callback.

#### `dns.resolveTxt(hostname, callback)` → `string[][]` (one inner array per TXT record; multiple chunks per record are individual strings). Variant: callback.

#### `dns.reverse(ip, callback)`

Reverse DNS lookup (PTR) for a v4/v6 address.

| Name | Type | Optional |
|---|---|---|
| `ip` | `string` | no |
| `callback` | `(err, hostnames: string[]) => void` | no |

Throws (sync): `ERR_INVALID_IP_ADDRESS` if `ip` is not valid. Variant: callback.

#### `dns.getServers()`

Returns: `string[]` — currently configured DNS servers, RFC 5952-formatted (IPv6 bracketed, non-default port appended). No args. Variant: sync.

#### `dns.setServers(servers)`

| Name | Type | Optional |
|---|---|---|
| `servers` | `readonly string[]` (each `'ip'`, `'ip:port'`, `'[ipv6]'`, `'[ipv6]:port'`) | no |

Return: `void`. Throws: `Error` (`ERR_INVALID_IP_ADDRESS` style) synchronously on a malformed entry; must not be called while a query is outstanding. Variant: sync.

#### `dns.setDefaultResultOrder(order)`

| Name | Type | Optional | Default |
|---|---|---|---|
| `order` | `'ipv4first' \| 'ipv6first' \| 'verbatim'` | no | — |

Return: `void`. Affects the process-wide default consulted by `dns.lookup()` (and `net.connect`), overridable per-call via `options.order`. Variant: sync.

#### `dns.getDefaultResultOrder()`

Return: `'ipv4first' | 'ipv6first' | 'verbatim'`. Variant: sync. (Note: this is also what `dns.lookup` internally consults; test code frequently mocks this.)

#### `dns.setLocalAddress([ipv4][, ipv6])`

Module-level equivalent of `Resolver#setLocalAddress`, sets the source IP address used for outgoing DNS requests on the default resolver.

| Name | Type | Optional | Default |
|---|---|---|---|
| `ipv4` | `string` | yes | `'0.0.0.0'` |
| `ipv6` | `string` | yes | `'::0'` |

Return: `void`. Throws: `ERR_INVALID_IP_ADDRESS` on malformed address. Variant: sync.

### `node:dns/promises` (a.k.a. `dns.promises`)

Every callback function above has a promise-returning mirror with the callback parameter dropped and the callback's non-error arguments folded into the resolved value (single value if one non-error callback arg, e.g. `reverse`; the natural shape otherwise, e.g. `lookup` resolves `{address, family}` or `LookupAddress[]` when `all: true`). All functions below reject with the same `NodeJS.ErrnoException`-shaped error the callback API passes as `err`.

| Function | Resolves to |
|---|---|
| `dnsPromises.lookup(hostname[, options])` | `{ address: string; family: number }` or `LookupAddress[]` if `all: true` |
| `dnsPromises.lookupService(address, port)` | `{ hostname: string; service: string }` |
| `dnsPromises.resolve(hostname[, rrtype])` | `string[] \| object[] \| object` (per rrtype) |
| `dnsPromises.resolve4(hostname[, options])` | `string[] \| Array<{address,ttl}>` |
| `dnsPromises.resolve6(hostname[, options])` | `string[] \| Array<{address,ttl}>` |
| `dnsPromises.resolveAny(hostname)` | `AnyRecord[]` |
| `dnsPromises.resolveCaa(hostname)` | `CaaRecord[]` |
| `dnsPromises.resolveCname(hostname)` | `string[]` |
| `dnsPromises.resolveMx(hostname)` | `MxRecord[]` |
| `dnsPromises.resolveNaptr(hostname)` | `NaptrRecord[]` |
| `dnsPromises.resolveNs(hostname)` | `string[]` |
| `dnsPromises.resolvePtr(hostname)` | `string[]` |
| `dnsPromises.resolveSoa(hostname)` | `SoaRecord` |
| `dnsPromises.resolveSrv(hostname)` | `SrvRecord[]` |
| `dnsPromises.resolveTlsa(hostname)` | `TlsaRecord[]` |
| `dnsPromises.resolveTxt(hostname)` | `string[][]` |
| `dnsPromises.reverse(ip)` | `string[]` |
| `dnsPromises.getServers()` | `string[]` (sync, not actually a promise) |
| `dnsPromises.setServers(servers)` | `void` (sync) |
| `dnsPromises.setDefaultResultOrder(order)` | `void` (sync) |
| `dnsPromises.getDefaultResultOrder()` | `'ipv4first'\|'ipv6first'\|'verbatim'` (sync) |
| `dnsPromises.setLocalAddress([ipv4][, ipv6])` | `void` (sync) |

### Properties & constants

#### Address-family / lookup hints (bitmask flags, combine with `|`)

| Constant | Meaning |
|---|---|
| `dns.ADDRCONFIG` | Only return address types configured on the local system (a non-loopback IPv4 address must be configured for A/IPv4 results; likewise IPv6) |
| `dns.V4MAPPED` | If IPv6 is requested but none found, return IPv4-mapped IPv6 addresses |
| `dns.ALL` | (used with `V4MAPPED`) return both real IPv6 addresses and IPv4-mapped IPv6 addresses |

Also exposed identically on `dns.promises` (`dnsPromises.ADDRCONFIG`, etc).

#### Error-code constants (all `number`, all also exposed on `dns.promises`)

| Constant | Meaning |
|---|---|
| `dns.NODATA` | DNS server returned an answer with no data |
| `dns.FORMERR` | DNS query misformatted |
| `dns.SERVFAIL` | DNS server returned general failure |
| `dns.NOTFOUND` | Domain name not found |
| `dns.NOTIMP` | DNS server does not implement the requested operation |
| `dns.REFUSED` | DNS server refused the query |
| `dns.BADQUERY` | Misformatted DNS query |
| `dns.BADNAME` | Misformatted host name |
| `dns.BADFAMILY` | Unsupported address family |
| `dns.BADRESP` | Misformatted DNS reply |
| `dns.CONNREFUSED` | Could not contact DNS servers |
| `dns.TIMEOUT` | Timeout while contacting DNS servers |
| `dns.EOF` | End of file |
| `dns.FILE` | Error reading file |
| `dns.NOMEM` | Out of memory |
| `dns.DESTRUCTION` | Channel is being destroyed |
| `dns.BADSTR` | Misformatted string |
| `dns.BADFLAGS` | Illegal flags specified |
| `dns.NONAME` | Given host name is not numeric |
| `dns.BADHINTS` | Illegal hints flags specified |
| `dns.NOTINITIALIZED` | c-ares library initialization not yet performed |
| `dns.LOADIPHLPAPI` | Error loading `iphlpapi.dll` (Windows) |
| `dns.ADDRGETNETWORKPARAMS` | Could not find the `GetNetworkParams` function (Windows) |
| `dns.CANCELLED` | DNS query cancelled |

These are numeric constants describing c-ares error categories; the string codes actually surfaced on thrown/rejected `Error.code` are the `E`-prefixed forms (`ENOTFOUND`, `ETIMEOUT`, `ECANCELLED`, `ECONNREFUSED`, `EFORMERR`, `ESERVFAIL`, `ENOTIMP`, `EREFUSED`, `EBADQUERY`, `EBADNAME`, `EBADFAMILY`, `EBADRESP`, `EBADSTR`, `EBADFLAGS`, `ENONAME`, `EBADHINTS`, `ENOTINITIALIZED`, `ELOADIPHLPAPI`, `EADDRGETNETWORKPARAMS`, `EFILE`, `ENOMEM`, `EDESTRUCTION`).

### Events

None. `dns.Resolver` and `dns.promises.Resolver` are plain classes, not `EventEmitter` subclasses — cancellation and errors surface via callback/promise rejection, not events.

## 3. Types & option objects

```typescript
interface ResolverOptions {
  timeout?: number;   // query timeout in ms; -1 = default (c-ares default, ~5000ms with retries)
  tries?: number;     // number of attempts the resolver makes before giving up; default: 4
  maxTimeout?: number; // maximum timeout between retries in ms; default: 0 (no cap beyond c-ares' own backoff)
}

interface LookupOptions {
  family?: 0 | 4 | 6 | 'IPv4' | 'IPv6'; // 0 = either
  hints?: number;      // bitmask of dns.ADDRCONFIG | dns.V4MAPPED | dns.ALL
  all?: boolean;       // default false; true => callback(err, addresses: LookupAddress[])
  verbatim?: boolean;  // deprecated since v22.1.0/v20.13.0; default true (no reordering)
  order?: 'ipv4first' | 'ipv6first' | 'verbatim'; // takes precedence over verbatim; default from dns.getDefaultResultOrder()
}

interface LookupAddress {
  address: string;
  family: number; // 4 | 6
}

interface LookupOneOptions extends LookupOptions {
  all?: false;
}

interface LookupAllOptions extends LookupOptions {
  all: true;
}

interface RecordWithTtl {
  address: string;
  ttl: number; // seconds
}

interface CaaRecord {
  critical: number;
  issue?: string;
  issuewild?: string;
  iodef?: string;
  contactemail?: string;
  contactphone?: string;
}

interface MxRecord {
  priority: number;
  exchange: string;
}

interface NaptrRecord {
  flags: string;
  service: string;
  regexp: string;
  replacement: string;
  order: number;
  preference: number;
}

interface SoaRecord {
  nsname: string;
  hostmaster: string;
  serial: number;
  refresh: number;
  retry: number;
  expire: number;
  minttl: number;
}

interface SrvRecord {
  priority: number;
  weight: number;
  port: number;
  name: string;
}

interface TlsaRecord {
  certUsage: number;
  selector: number;
  match: number;
  data: ArrayBuffer; // raw certificate association data
}

// dns.resolveAny() element union, discriminated by `type`
type AnyRecord =
  | ({ type: 'A' } & RecordWithTtl)
  | ({ type: 'AAAA' } & RecordWithTtl)
  | { type: 'CNAME'; value: string }
  | ({ type: 'MX' } & MxRecord)
  | ({ type: 'NAPTR' } & NaptrRecord)
  | { type: 'NS'; value: string }
  | { type: 'PTR'; value: string }
  | ({ type: 'SOA' } & SoaRecord)
  | ({ type: 'SRV' } & SrvRecord)
  | { type: 'TXT'; entries: string[] };

interface Ip4ResolveOptions {
  ttl: boolean; // default false
}
```

Callback shapes (already given inline per-function in section 2) all follow the Node convention `(err: NodeJS.ErrnoException | null, ...results) => void`. The error object, when present, has:

```typescript
interface DnsErrnoException extends Error {
  code?: string;     // e.g. 'ENOTFOUND', 'ETIMEOUT', 'ECANCELLED'
  errno?: number;     // negative libuv/c-ares errno
  syscall?: string;   // e.g. 'getaddrinfo', 'queryA', 'queryTxt', 'queryCname'
  hostname?: string;  // hostname being resolved when the failure occurred
}
```

## 4. Node semantics & edge cases

- **Two independent resolution paths.** `dns.lookup()`/`dns.lookupService()` use OS facilities (`getaddrinfo(3)`/`getnameinfo(3)`) on **libuv's threadpool** — they consult `/etc/hosts`, `nsswitch.conf(5)`, `resolv.conf(5)` on POSIX, and the Windows resolver stack on Windows, and their behavior matches OS utilities like `ping`. Excess concurrent `dns.lookup()` calls can exhaust `UV_THREADPOOL_SIZE` and stall unrelated threadpool-bound work (fs, crypto). `dns.resolve()`/`dns.resolve*()`/`dns.reverse()` instead go straight to the DNS protocol over the network via the bundled c-ares library, are fully async without touching the threadpool, and do **not** consult `/etc/hosts` — they always hit the network/configured servers.
- **`family: 0` / `'IPv4'`/`'IPv6'` string aliases.** Since Node 18.4.0, `family` accepts `'IPv4'`/`'IPv6'` string aliases for `net` compatibility in addition to `0|4|6`. `0` means "either family, OS default order/preference".
- **`verbatim` vs `order`.** `verbatim` (boolean) is deprecated since v22.1.0/v20.13.0 in favor of `order: 'ipv4first'|'ipv6first'|'verbatim'`; when both are given, `order` wins. Default has been `verbatim: true` (no reordering) since Node 17; before that, results were reordered IPv4-first by default. `dns.setDefaultResultOrder()`/`dns.getDefaultResultOrder()` control the process-wide default (`'verbatim'` unless set), and it can also be set via the `--dns-result-order` CLI flag (the JS-level setter has priority over the CLI flag).
- **`dns.resolveAny()` is not reliable.** `ANY` is not itself a DNS RR type in the send/response sense that guarantees completeness — some servers omit e.g. AAAA even when present. Node's docs explicitly discourage relying on it in new code; prefer the specific `resolve4`/`resolve6`/etc.
- **`dns.setServers()` semantics.** Completely replaces the current server list (no incremental add); throws synchronously on a malformed entry; must not be called while a DNS query is in flight (undefined behavior otherwise); only affects `resolve*`/`reverse`, never `lookup`. If the first configured server answers `NOTFOUND`, subsequent servers are **not** tried as fallback — fallback only kicks in on timeout or other error classes, not on an authoritative negative answer. Server strings support `ip`, `ip:port`, `[ipv6]`, `[ipv6]:port` (RFC 5952 formatting on `getServers()` output).
- **Worker-thread isolation.** DNS server list, default result order, and local address are all **not** inherited/propagated across `worker_threads` — a change on the main thread does not affect an already-running or new worker; each worker starts from the same base OS/CLI configuration independently.
- **IPv6 scope IDs / zone identifiers** (e.g. `fe80::1%eth0`) are accepted where the OS resolver supports them for `lookup`/`reverse`; `resolve*` c-ares paths generally operate on global addresses.
- **`Resolver#cancel()`** cancels all outstanding requests started by that resolver instance; each pending callback receives an error with `code: 'ECANCELLED'` (promise variant: rejects with the same code). Calling `cancel()` on the default global functions is not possible directly — only via an explicit `Resolver`/`dns.promises.Resolver` instance.
- **Error object shape.** `err.code` is the primary `E`-prefixed string (`ENOTFOUND`, `ETIMEOUT`, `ECONNREFUSED`, `ECANCELLED`, …), `err.errno` a numeric code, `err.syscall` names the underlying operation (`'getaddrinfo'`, `'queryA'`, `'queryMx'`, `'queryTxt'`, `'queryCname'`, `'queryMx'`, `'querySrv'`, `'queryMailB'`, `'resolveAny'`, etc.), `err.hostname` the queried name. `ENOTFOUND` is generic and used for most "no record for this rrtype"/"could not resolve" cases.
- **Windows vs POSIX.** `dns.lookup()`'s underlying `getaddrinfo` on Windows may consult the `hosts` file, DNS Client service cache, and configured adapters differently than glibc's NSS chain on Linux; results ordering nuances (`ipv4first`/`ipv6first`) can differ subtly by platform even with the same `order` requested, since the OS resolver still applies its own tie-breaking for `lookup` (only `resolve*`, driven entirely by c-ares, is platform-uniform). `LOADIPHLPAPI`/`ADDRGETNETWORKPARAMS` error constants are Windows-specific (failure to load `iphlpapi.dll` / find `GetNetworkParams`).
- **Deprecations.** `verbatim` boolean option deprecated (see above) — do not remove support, but implement `order` as the primary/preferred path. No hard-removed APIs in this module as of Node 25; `dns.promises` has been stable since v11.14.0 (introduced v10.6.0).
- **No backpressure concerns** — this module has no streams; every call is a bounded request/response.
- **Security notes.** DNS responses are attacker-influenceable if the configured resolver/network path is untrusted (cache poisoning, rebinding); RTS should not add trust beyond what the OS/c-ares-equivalent implementation already provides. `dns.lookup()` results should not be treated as a security boundary (classic DNS rebinding concern also documented for `node:net`/`node:http`).

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node` is a fully independent crate (no `rts-std` dependency). It owns two resolution backends internally, mirroring Node's own split:

- **`lookup`/`lookupService` (OS-facility path).** Backed by Rust `std::net::ToSocketAddrs` / a small `getaddrinfo`/`getnameinfo` FFI wrapper (`libc::getaddrinfo` on Unix, `winapi`/`windows-sys` `GetAddrInfoW`/`GetNameInfoW` on Windows) run on a bounded blocking thread pool owned by `rts-node` (see 5.3/5.7 — this pool must NOT be the shared tokio blocking pool if that pool lives in `rts-std`; either `rts-node` spins its own small `std::thread` pool, or the blocking-pool primitive is hoisted to a shared low crate both can use).
- **`resolve*`/`reverse`/`Resolver` (DNS-protocol path).** Backed by the `hickory-resolver` crate (formerly `trust-dns-resolver`) or `c-ares`/`c-ares-resolver` bindings — `hickory-resolver` is preferred as a pure-Rust, no-libc-dependency implementation that matches c-ares' async, non-threadpool behavior and is easy to vendor per-target for cross-compilation. This gives native A/AAAA/CNAME/MX/TXT/NS/PTR/SOA/SRV/NAPTR/CAA/TLSA/ANY query support and configurable server lists/timeouts/tries without a c-ares C dependency.
- Address/IP parsing and validation (`ERR_INVALID_IP_ADDRESS`) uses `std::net::IpAddr::from_str`.
- Error-code mapping: a small table in `rts-node/src/dns/errors.rs` maps `hickory-resolver` `ResolveErrorKind` / `std::io::ErrorKind` to the Node `E*` string codes and the numeric `dns.<CODE>` constants.

### 5.2 ABI surface

Symbol convention: `__RTS_FN_NODE_DNS_<NAME>`. Rich/stateful objects (a `Resolver` instance, and any in-flight cancellable query set) are opaque `Handle` (u64) values into an `rts-node`-owned `HandleTable`-style slab (or the shared `rts-engine` `HandleTable` if hoisted — see 5.7). Everything else (hostnames, IPs, record fields) crosses as `StrPtr`/`I64`/`F64`/`Bool` primitives; compound results (arrays of records) are built by the `.ts` shim from repeated primitive-returning calls or a single JSON-encoded `StrPtr` blob decoded in `.ts` (JSON path is simplest for the many small heterogeneous record shapes: MX/SRV/SOA/NAPTR/CAA/TLSA).

| Symbol | Args (AbiType) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_DNS_LOOKUP` | `StrPtr hostname, I32 family, I32 hints, Bool all, I32 order` | `Handle` (query handle; async) | resolves via promise subsystem, see 5.3 |
| `__RTS_FN_NODE_DNS_LOOKUP_SERVICE` | `StrPtr address, I32 port` | `Handle` | |
| `__RTS_FN_NODE_DNS_RESOLVE` | `StrPtr hostname, StrPtr rrtype` | `Handle` | dispatches internally by rrtype |
| `__RTS_FN_NODE_DNS_RESOLVE4` | `StrPtr hostname, Bool ttl` | `Handle` | |
| `__RTS_FN_NODE_DNS_RESOLVE6` | `StrPtr hostname, Bool ttl` | `Handle` | |
| `__RTS_FN_NODE_DNS_RESOLVE_ANY` | `StrPtr hostname` | `Handle` | |
| `__RTS_FN_NODE_DNS_RESOLVE_CAA` | `StrPtr hostname` | `Handle` | |
| `__RTS_FN_NODE_DNS_RESOLVE_CNAME` | `StrPtr hostname` | `Handle` | |
| `__RTS_FN_NODE_DNS_RESOLVE_MX` | `StrPtr hostname` | `Handle` | |
| `__RTS_FN_NODE_DNS_RESOLVE_NAPTR` | `StrPtr hostname` | `Handle` | |
| `__RTS_FN_NODE_DNS_RESOLVE_NS` | `StrPtr hostname` | `Handle` | |
| `__RTS_FN_NODE_DNS_RESOLVE_PTR` | `StrPtr hostname` | `Handle` | |
| `__RTS_FN_NODE_DNS_RESOLVE_SOA` | `StrPtr hostname` | `Handle` | |
| `__RTS_FN_NODE_DNS_RESOLVE_SRV` | `StrPtr hostname` | `Handle` | |
| `__RTS_FN_NODE_DNS_RESOLVE_TLSA` | `StrPtr hostname` | `Handle` | |
| `__RTS_FN_NODE_DNS_RESOLVE_TXT` | `StrPtr hostname` | `Handle` | |
| `__RTS_FN_NODE_DNS_REVERSE` | `StrPtr ip` | `Handle` | |
| `__RTS_FN_NODE_DNS_GET_SERVERS` | (none) | `StrPtr` (JSON array) | sync |
| `__RTS_FN_NODE_DNS_SET_SERVERS` | `StrPtr serversJson` | `Void` | sync, throws via thread-local error slot on bad entry |
| `__RTS_FN_NODE_DNS_SET_DEFAULT_RESULT_ORDER` | `StrPtr order` | `Void` | sync |
| `__RTS_FN_NODE_DNS_GET_DEFAULT_RESULT_ORDER` | (none) | `StrPtr` | sync |
| `__RTS_FN_NODE_DNS_SET_LOCAL_ADDRESS` | `StrPtr ipv4, StrPtr ipv6` | `Void` | sync |
| `__RTS_FN_NODE_DNS_QUERY_AWAIT` | `Handle query` | `StrPtr` (JSON result) or error via slot | blocking join used by both callback-adapter and promise path |
| `__RTS_FN_NODE_DNS_RESOLVER_NEW` | `I32 timeoutMs, I32 tries, I32 maxTimeoutMs` | `Handle` (resolver) | |
| `__RTS_FN_NODE_DNS_RESOLVER_FREE` | `Handle resolver` | `Void` | |
| `__RTS_FN_NODE_DNS_RESOLVER_RESOLVE*` (one per rrtype, mirrors module-level) | `Handle resolver, StrPtr hostname[, ...opts]` | `Handle` (query) | scoped to that resolver's server list/timeout |
| `__RTS_FN_NODE_DNS_RESOLVER_REVERSE` | `Handle resolver, StrPtr ip` | `Handle` | |
| `__RTS_FN_NODE_DNS_RESOLVER_GET_SERVERS` | `Handle resolver` | `StrPtr` (JSON) | |
| `__RTS_FN_NODE_DNS_RESOLVER_SET_SERVERS` | `Handle resolver, StrPtr serversJson` | `Void` | |
| `__RTS_FN_NODE_DNS_RESOLVER_SET_LOCAL_ADDRESS` | `Handle resolver, StrPtr ipv4, StrPtr ipv6` | `Void` | |
| `__RTS_FN_NODE_DNS_RESOLVER_CANCEL` | `Handle resolver` | `Void` | cancels all in-flight queries for that resolver; each pending `QUERY_AWAIT` completes with `ECANCELLED` |

Why per-query `Handle` + a separate `QUERY_AWAIT`: it lets the same native call serve both the callback-style `.ts` shim (spawn query, register a promise-subsystem continuation that invokes the user callback on settle) and the `dns.promises` shim (spawn query, `await promise.wait(handle)`-equivalent), without duplicating the resolver logic per style — matching the existing async/Promise/Function design (`docs/specs/async-promise-function.md`).

The record-array/record-object results (MX/SRV/SOA/NAPTR/CAA/TLSA/ANY/lookupService/lookup-all) are returned as a single JSON `StrPtr` decoded by a tiny `.ts`-side JSON.parse, rather than one ABI call per field — this avoids inventing N bespoke marshalling shapes for record types that only exist in this module and keeps the native surface small; `TLSA`'s `data` field (raw bytes) is base64-encoded inside that JSON and reinflated to a `Uint8Array`/`ArrayBuffer` on the `.ts` side (see 5.5).

### 5.3 Async model

- **`lookup`/`lookupService`** (OS threadpool path): native call spawns the blocking `getaddrinfo`/`getnameinfo` work on a dedicated blocking-work pool, then settles a Promise/invokes the callback exactly like the rest of RTS's async surface — i.e. it still goes through the shared promise subsystem for the JS-facing completion, only the actual OS call is blocking-executed off the main native thread.
- **`resolve*`/`reverse`/`Resolver.*`** (DNS-protocol path): `hickory-resolver`'s resolver is natively async (Tokio-based). The query future is spawned on the shared tokio runtime and its completion resolves/rejects the associated Promise, mirroring `promise.create`/`promise.wait` from the async/Promise/Function design.
- **Callback-style calls** (`dns.lookup(host, cb)`, `dns.resolve4(host, cb)`, …) are a `.ts` shim over the promise-returning native call: `nativeResolve(...).then(v => cb(null, v), e => cb(e))` — Node's callback API and promise API share one native implementation, exactly like the promise-centric design already used elsewhere in RTS.
- **`Resolver#cancel()` / `dns.promises.Resolver#cancel()`**: native call marks all in-flight query handles owned by that resolver as cancelled (drops/aborts the underlying tokio tasks or hickory-resolver futures) and settles their pending promises/callbacks with an `ECANCELLED`-coded error.
- **Sync calls** (`getServers`, `setServers`, `setDefaultResultOrder`, `getDefaultResultOrder`, `setLocalAddress`) call directly into a small `Mutex`/`RwLock`-guarded config struct with no async involvement.

### 5.4 Multithread / worker interaction

- Per Node semantics (4 above), DNS configuration (server list, default result order, local address) is **not** shared across worker threads — each RTS thread/region gets its own independent `DnsConfig` state (servers, order, local address), initialized from the OS/CLI defaults, never promoted to the shared heap. This maps directly onto the RTS threading model's `threadLocal` region: `DnsConfig` lives per-thread, analogous to how per-module state elsewhere in the runtime uses `thread_local!`/independent `Arc<Mutex<T>>` instances that are not shared.
- A `dns.Resolver`/`dns.promises.Resolver` instance is itself **not** meant to be shared across threads in Node (it's a JS-heap object); in RTS it should be an opaque `Handle` whose backing native resolver state also lives in the owning thread's region — if a `Resolver` handle is passed across a `worker_threads` `MessagePort`/channel, RTS should either reject it (structured-clone-style, matching Node's own non-transferability of live resolver objects) or transparently construct an equivalent resolver in the target thread from serialized options (servers/timeout/tries) — not literally share the underlying c-ares/hickory-resolver state, since Node itself never does.
- In-flight queries (`Handle query`) are inherently tied to the tokio task that produces their result; the `HandleTable`/slab entry for a query is safe to be read from any thread once settled (the value itself, once written, is immutable), consistent with the existing shard-aware `HandleTable` design (32 lock-free shards, round-robin alloc).
- No shared-memory (`SharedArrayBuffer`) concerns in this module — no raw buffers are exchanged except the small `TLSA` certificate-association byte blob, which is copied (not shared) into a fresh `ArrayBuffer` per read.

### 5.5 Buffer / TypedArray interop

The only byte-data surface in this module is `TlsaRecord.data: ArrayBuffer`. The native side returns the raw association bytes base64-encoded inside the query's JSON result payload (`StrPtr`); the `.ts` shim base64-decodes into a fresh `Uint8Array`/`ArrayBuffer` (both primordial, engine-owned memory model) before handing the record object to user code. No `Buffer`-specific API is needed here (`Buffer extends Uint8Array` per the engine's model, so nothing module-specific is required) — this keeps the native `extern "C"` surface free of any bespoke binary-transfer ABI type, at the cost of one base64 round-trip for the (rare) TLSA case only.

### 5.6 Doctrine placement

`node:dns` is **non-primordial** — the engine (`rts-codegen-new`) must never hardcode `"dns"` or any of its member names. Resolution works exactly like every other `node:` module: `import ... from 'node:dns'` (or `'node:dns/promises'`) is mapped through `rts_node::ns_prefix_for("node:dns")` → `"node_dns"` (data lookup against `NODE_SPECS`, no hardcoded arm in codegen), and each call `node_dns.resolve4(...)` resolves via `rts_node::node_lookup("node_dns.resolve4")` to a `NodespaceMember` (`symbol`, `args`, `returns`) — purely data-driven, matching the existing `NodespaceSpec`/`NODE_SPECS`/`node_lookup` mechanism already implemented in `crates/rts-node/src/lib.rs`. `node:dns/promises` is a second `NodespaceSpec` (`node_module: "dns/promises"`, distinct `ns_prefix`, e.g. `"node_dns_promises"`) so the callback and promise surfaces can have distinct ABI members without the engine ever branching on the string `"dns"`.

The native-extern / `.ts`-shim split: every symbol in 5.2 is a raw primitive (query-handle spawn, JSON-blob fetch, sync config get/set). All JS-shape ergonomics — the `Resolver` class wrapper, option-object normalization (`{ttl: true}` vs `{ttl: false}` overload resolution, `hints` bitmask assembly from `dns.ADDRCONFIG|dns.V4MAPPED|dns.ALL`), the callback-vs-promise dual API, JSON decoding of record arrays into properly-shaped JS objects, and the error-code constant objects (`dns.NODATA`, `dns.ADDRCONFIG`, etc., which are plain numeric `.ts` constants, not natively computed) — live in a `.ts` shim shipped by `rts-node` (e.g. `rts-node/src/dns/dns.ts` + `dns_promises.ts` + `resolver.ts`).

### 5.7 Shared-infra dependencies (FLAG)

- **Promise/async settle subsystem.** Every non-sync function in this module (`lookup`, `lookupService`, all `resolve*`, `reverse`, and the `Resolver` instance methods) needs the promise-create/settle machinery (`promise.create`/`promise.wait`-equivalent) that currently lives in `rts-std` (`promise` namespace) per `docs/specs/async-promise-function.md`. Since `rts-node` cannot depend on `rts-std`, this must be hoisted into `rts-engine` (or a new shared low crate both `rts-primitives`/`rts-shared` and `rts-node` can depend on) before `node:dns` (or any other async node module) can be implemented for real.
- **Shared tokio runtime (`rt()` in `rts-runtime/src/runtime/async_rt.rs`).** The DNS-protocol path (`hickory-resolver`) is tokio-native and the OS-facility path (`lookup`) needs a blocking-task pool; both currently assume the single global multi-thread tokio runtime that lives under `rts-std`/`rts-runtime`. This runtime accessor needs to be reachable from `rts-node` without a `rts-std` dependency — same hoist as above.
- **GC thread-registry hooks (`on_thread_start`/`on_thread_stop`).** Any tokio worker running DNS queries must be registered in `gc/thread_registry` so the GC's conservative stack scanner sees live handles held by in-flight queries; this registration hook currently lives alongside the shared tokio runtime setup in `rts-std`/`rts-runtime` and must be reachable the same way.
- **HandleTable.** Query handles and `Resolver` handles need a `HandleTable`-shaped slab (shard-aware, gen16+slot48). If `rts-node` cannot depend on the concrete `HandleTable` in `rts-engine`'s runtime-facing crate, it needs either (a) direct access to `rts-engine`'s `HandleTable` (likely fine — `rts-engine` is the lowest layer and primordial-adjacent), or (b) its own independent slab implementation duplicating the shard logic. Prefer (a): confirm `rts-engine::HandleTable` is importable from `rts-node` without pulling in `rts-std`.
- **No TLS/crypto dependency** — `node:dns` itself does not need `rustls`/crypto primitives (DNS-over-TLS/DoH are out of scope for Node's `dns` module; plain UDP/TCP DNS only).
- **Net/socket primitives.** The DNS-protocol path needs raw UDP/TCP socket I/O to talk to configured DNS servers; if `hickory-resolver` is used, it brings its own tokio-net-based transport and does not need `rts-std`'s `net`/`tls` namespaces — this is the one piece of "shared infra" this module can avoid needing hoisted, by choosing a self-contained resolver crate instead of reusing `rts-std::net`.

### 5.8 Implementation phases

1. **(a)** Add `rts-node/src/dns/mod.rs` with the `NodespaceSpec` skeleton (`node_module: "dns"`, `ns_prefix: "node_dns"`) and a parallel `dns_promises` spec (`node_module: "dns/promises"`); register both in `NODE_SPECS`.
2. **(b)** Implement sync config surface first (no async infra needed): `getServers`/`setServers`/`setDefaultResultOrder`/`getDefaultResultOrder`/`setLocalAddress`, backed by a `Mutex<DnsConfig>` (thread-local per 5.4). Add the `dns.ADDRCONFIG`/`V4MAPPED`/`ALL` and error-code numeric constants as `.ts` constants (no native symbol needed — plain literals).
3. **(c)** Resolve the 5.7 blocker: hoist (or confirm reachability of) the promise/settle subsystem, shared tokio runtime accessor, and GC thread-registry hooks so `rts-node` can use them without an `rts-std` dependency. This is a prerequisite for every remaining phase.
4. **(d)** Implement `reverse` and `resolve4`/`resolve6` (the highest-traffic record types) end-to-end: native query spawn → JSON result → `.ts` decode → callback-and-promise dual shim. Establishes the query-handle + `QUERY_AWAIT` pattern reused by everything else.
5. **(e)** Implement the remaining `resolve*` record types (`Cname`, `Mx`, `Ns`, `Ptr`, `Soa`, `Srv`, `Naptr`, `Caa`, `Txt`, `Tlsa`, `Any`) — mostly mechanical repeats of (d)'s pattern with different result-shape decoding.
6. **(f)** Implement `lookup`/`lookupService` (OS-facility path) with the blocking-pool dispatch; wire `hints`/`family`/`order`/`all` option handling in the `.ts` shim.
7. **(g)** Implement the `dns.Resolver` class: native `RESOLVER_NEW`/`RESOLVER_FREE` + per-resolver variants of every `resolve*`/`reverse`/`getServers`/`setServers`/`setLocalAddress`, plus `cancel()`.
8. **(h)** Implement `dns.promises.Resolver` as a thin `.ts` wrapper reusing the same native `Resolver` handle calls (promise-native calls need no adaptation; only the constructor/class wrapper differs from the callback `Resolver`).
9. **(i)** Wire `node:dns/promises` as its own importable specifier resolving to the `dns_promises` `NodespaceSpec`, and `dns.promises` as a property on the `node:dns` `.ts` shim's default export pointing at the same underlying functions.

## 6. Test plan

```
tests/node/dns/dns_lookup_basic.test.ts
  - dns.lookup('localhost', cb) resolves to 127.0.0.1 or ::1, family 4 or 6
  - dns.lookup('localhost', { family: 4 }, cb) forces IPv4
  - dns.lookup('localhost', { family: 'IPv6' }, cb) forces IPv6 (string alias)
  - dns.lookup('localhost', { all: true }, cb) returns an array of {address, family}
  - dns.lookup('this-domain-does-not-exist.invalid', cb) => err.code === 'ENOTFOUND'
  - dns.lookup(123 as any, cb) => throws ERR_INVALID_ARG_TYPE synchronously

tests/node/dns/dns_lookup_service.test.ts
  - dns.lookupService('127.0.0.1', 80, cb) resolves hostname+service
  - dns.lookupService('bad-ip', 80, cb) => err set

tests/node/dns/dns_resolve_records.test.ts
  - dns.resolve4('example.com', cb) => string[] of IPv4 addrs
  - dns.resolve4('example.com', { ttl: true }, cb) => Array<{address, ttl}>
  - dns.resolve6('example.com', cb) => string[] of IPv6 addrs
  - dns.resolveMx('example.com', cb) => Array<{priority, exchange}>, sorted by priority where server supports it
  - dns.resolveTxt('example.com', cb) => string[][]
  - dns.resolveNs('example.com', cb) => string[] of nameservers
  - dns.resolveCname('www.example.com', cb) => string[]
  - dns.resolveSoa('example.com', cb) => single SoaRecord object (not array)
  - dns.resolveSrv/_naptr/_caa/_tlsa smoke tests against a known-record test domain
  - dns.resolveAny('example.com', cb) => array of tagged records; assert warning/best-effort semantics (no strict completeness assertion)
  - dns.resolve('example.com', 'MX', cb) dispatches identically to resolveMx
  - unknown/NXDOMAIN hostname => err.code === 'ENOTFOUND' or 'ESERVFAIL' across all resolve* variants

tests/node/dns/dns_reverse.test.ts
  - dns.reverse('8.8.8.8', cb) => hostnames array
  - dns.reverse('not-an-ip', cb) => throws ERR_INVALID_IP_ADDRESS synchronously

tests/node/dns/dns_servers_config.test.ts
  - dns.getServers() returns non-empty array by default
  - dns.setServers(['8.8.8.8', '[2001:4860:4860::8888]:1053']) then getServers() reflects it
  - dns.setServers(['not-an-ip']) throws synchronously
  - dns.setDefaultResultOrder('ipv4first'); dns.getDefaultResultOrder() === 'ipv4first'
  - dns.setDefaultResultOrder('bogus') throws
  - dns.setLocalAddress('127.0.0.1') does not throw; malformed address throws ERR_INVALID_IP_ADDRESS

tests/node/dns/dns_promises.test.ts
  - import dnsPromises from 'node:dns/promises'; await dnsPromises.resolve4('example.com') => string[]
  - await dnsPromises.lookup('localhost') => {address, family}
  - rejected promise on ENOTFOUND carries .code
  - dnsPromises.getServers()/setServers() sync behavior matches callback API

tests/node/dns/dns_resolver_class.test.ts
  - new dns.Resolver({ timeout: 2000, tries: 2 }); resolver.setServers([...]); resolver.resolve4('example.com', cb) uses only configured servers (mock/stub server or known-authoritative test domain)
  - resolver.cancel() causes all pending resolve4/resolveMx/etc callbacks to receive err.code === 'ECANCELLED'
  - resolver.getServers() reflects resolver-local config, independent of global dns.setServers()
  - new dns.promises.Resolver(...) mirrors the same behavior with promise rejection on cancel

tests/node/dns/dns_edge_cases.test.ts
  - concurrent dns.lookup() calls (e.g. 50 in parallel) all resolve without deadlocking the threadpool
  - dns.lookup with hints: dns.ADDRCONFIG | dns.V4MAPPED does not throw and returns a valid shape
  - verbatim (deprecated) vs order both specified => order wins
  - dns.resolveTxt record with multiple TXT chunks => inner array has >1 element for that record
  - dns.resolveTlsa data field decodes to a Uint8Array/ArrayBuffer of expected length

tests/node/dns/dns_worker_threads.test.ts (multithread)
  - main thread dns.setServers([...]) does NOT affect a spawned worker_thread's dns.getServers() (isolation)
  - main thread dns.setDefaultResultOrder('ipv6first') does NOT affect a worker's dns.getDefaultResultOrder()
  - two Resolver instances in two different worker threads operate independently and concurrently without interfering (stress: N concurrent resolve4 calls across M workers, assert per-worker result integrity)
```

## 7. Open questions / deferrals

- **DNS backend choice (`hickory-resolver` vs raw c-ares FFI vs vendoring c-ares as a C dependency).** `hickory-resolver` avoids a C toolchain dependency and eases cross-compilation, but its error taxonomy and retry/backoff behavior are not byte-identical to c-ares; some edge-case error codes (`BADHINTS`, `NOTINITIALIZED`, the Windows-specific `LOADIPHLPAPI`/`ADDRGETNETWORKPARAMS`) may not have a natural c-ares-free equivalent and would need best-effort mapping or to simply never fire. Needs an owner decision before phase (c)/(d).
- **Exact fallback-server semantics** ("first server NOTFOUND does not try next, but other errors do") — needs to be validated against whatever backend is chosen; `hickory-resolver`'s built-in fallback policy may not match this exactly and might need a thin custom retry wrapper.
- **`dns.resolveAny()` fidelity** — deferred to best-effort; Node itself disclaims completeness, so RTS should not over-invest in matching c-ares' exact partial-result quirks here.
- **Promise/tokio/HandleTable hoist (5.7)** is a hard prerequisite shared with every other async node module (`node:fs/promises`, `node:net`, `node:timers/promises`, etc.) — this should likely be tracked as one cross-cutting infra task rather than solved per-module; flagging here so whoever picks up `node:dns` doesn't duplicate the hoist.
- **DNS-over-TLS/DoH** is out of scope — Node's `dns` module does not support it either; no deferral needed, just noting it's intentionally absent.
- **IPv6 zone/scope-id (`%eth0`) support** in the `lookup`/`reverse` OS-facility path depends on how much of `getaddrinfo`/`getnameinfo`'s platform-specific behavior the FFI wrapper chooses to expose; flagged as verify-on-implementation rather than blocking the spec.
- **`Resolver` instances crossing `worker_threads` channels** (structured-clone semantics for a `Resolver` handle) — Node itself does not really define this (resolvers are plain objects with methods, not natively cloneable in a meaningful way beyond generic object clone), so RTS's exact behavior here (reject vs. reconstruct-from-options) is an implementation choice, not a Node-parity requirement; flagged as an open design call for phase (g).
