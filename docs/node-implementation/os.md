# node:os

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:os` |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P0 |
| Status | [ ] Not implemented — spec only |
| Import forms | `import os from 'node:os'`; `import { cpus, platform, EOL } from 'node:os'`; `const os = require('node:os')` |
| Globals exposed | none (all access is via the `node:os` module import; no ambient globals) |

## 1. Purpose

`node:os` exposes host operating-system information and low-level system primitives that do not fit anywhere else: CPU topology and utilization counters, memory totals, network interface enumeration, user/home/temp-directory identity, process scheduling priority, load average, and a large table of platform-specific numeric constants (POSIX/Windows `errno` codes, POSIX signal numbers, `dlopen` flags, and Node's own priority-class enum). Every function in this module is a **synchronous, read-mostly query** against the host OS — there are no classes, no events, no callbacks, and no promises anywhere in the surface. It is the module every other "what platform am I on" decision in a Node program (and in RTS's own `node:*` compat layer) ultimately bottoms out on.

## 2. Exported API surface (COMPLETE)

### Classes

None. `node:os` exports only functions, two data properties (`EOL`, `devNull`), and the `constants` namespace object. There is no `EventEmitter` subclass, no constructor, and no events anywhere in this module.

### Properties

#### `os.EOL`

- **Type:** `string`
- **Value:** `'\n'` on POSIX, `'\r\n'` on Windows.
- **Note:** fixed at the time the RTS runtime binary was built for its target platform (not detected at process-start time) — same as Node, which bakes this into the compiled binary per platform.

#### `os.devNull`

- **Type:** `string`
- **Value:** `/dev/null` on POSIX, `\\.\nul` on Windows.
- **Added:** v22.9.0/v20.18.0.

#### `os.constants`

- **Type:** `object`
- **Description:** container object for the five constant groups below (`signals`, `errno`, `dlopen`, `priority`, `libuv`). See "Properties & constants" further down for the full member listing of each group.

### Top-level functions

| Function | Variant |
|---|---|
| `os.arch()` | sync |
| `os.availableParallelism()` | sync |
| `os.cpus()` | sync |
| `os.endianness()` | sync |
| `os.freemem()` | sync |
| `os.getPriority([pid])` | sync |
| `os.homedir()` | sync |
| `os.hostname()` | sync |
| `os.loadavg()` | sync |
| `os.machine()` | sync |
| `os.networkInterfaces()` | sync |
| `os.platform()` | sync |
| `os.release()` | sync |
| `os.setPriority([pid, ]priority)` | sync |
| `os.tmpdir()` | sync |
| `os.totalmem()` | sync |
| `os.type()` | sync |
| `os.uptime()` | sync |
| `os.userInfo([options])` | sync |
| `os.version()` | sync |

#### `os.arch()`

- **Added:** v0.5.0
- **Params:** none.
- **Returns:** `string` — one of `'arm'`, `'arm64'`, `'ia32'`, `'loong64'`, `'mips'`, `'mipsel'`, `'ppc64'`, `'riscv64'`, `'s390x'`, `'x64'`.
- **Throws:** never.
- **Note:** equivalent to `process.arch`; determined by the RTS binary's compiled target, not runtime detection.

#### `os.availableParallelism()`

- **Added:** v19.4.0, v18.14.0
- **Params:** none.
- **Returns:** `integer` (always `> 0`) — an estimate of the parallelism a program should use, respecting cgroup/container CPU quota and CPU-affinity masks where the OS exposes them (this is what differentiates it from `os.cpus().length`, which always reports every logical core regardless of quota).
- **Throws:** never.

#### `os.cpus()`

- **Added:** v0.3.3
- **Params:** none.
- **Returns:** `CpuInfo[]` — one entry per logical CPU core; may be an empty array on platforms/containers where per-core detail is unavailable.
- **Throws:** never (returns `[]` on failure rather than throwing).

#### `os.endianness()`

- **Added:** v0.9.4
- **Params:** none.
- **Returns:** `'BE' | 'LE'`.
- **Throws:** never. Fixed per compiled target (all targets RTS ships for today are little-endian, but the surface must not hardcode `'LE'`).

#### `os.freemem()`

- **Added:** v0.3.3
- **Params:** none.
- **Returns:** `integer` — free system memory in bytes.
- **Throws:** never.

#### `os.getPriority([pid])`

- **Added:** v10.10.0
- **Params:**

  | Name | Type | Optional | Default |
  |---|---|---|---|
  | `pid` | `integer` | yes | `0` (meaning the calling process) |

- **Returns:** `integer` — current scheduling priority of the process, range `-20` (highest) to `19` (lowest), per `os.constants.priority`.
- **Throws:** `SystemError` (`ESRCH` if no process with the given `pid` exists; `EPERM` if the caller lacks permission to inspect it).

#### `os.homedir()`

- **Added:** v2.3.0
- **Params:** none.
- **Returns:** `string` — current user's home directory.
- **Throws:** never (falls back to OS APIs if env vars are unset).
- **POSIX:** `$HOME` if set, else `getpwuid_r(3)`-style lookup by effective UID.
- **Windows:** `USERPROFILE` if set, else composed from the profile directory APIs.

#### `os.hostname()`

- **Added:** v0.3.3
- **Params:** none.
- **Returns:** `string` — the OS hostname.
- **Throws:** never.

#### `os.loadavg()`

- **Added:** v0.3.3
- **Params:** none.
- **Returns:** `[number, number, number]` — 1, 5, and 15-minute load averages.
- **Throws:** never.
- **Note:** Unix-specific concept; returns `[0, 0, 0]` unconditionally on Windows (no OS equivalent).

#### `os.machine()`

- **Added:** v18.9.0, v16.18.0
- **Params:** none.
- **Returns:** `string` — one of `'arm'`, `'arm64'`, `'aarch64'`, `'mips'`, `'mips64'`, `'ppc64'`, `'ppc64le'`, `'s390x'`, `'i386'`, `'i686'`, `'x86_64'` (raw `uname -m`/equivalent value; distinct enum from `os.arch()`'s Node-normalized value).
- **Throws:** never.

#### `os.networkInterfaces()`

- **Added:** v0.6.0
- **Params:** none.
- **Returns:** `Record<string, NetworkInterfaceInfo[]>` — keyed by interface name (e.g. `'lo'`, `'eth0'`, `'Ethernet'`); each value is an array (multiple addresses per interface, e.g. one IPv4 + one IPv6).
- **Throws:** never (returns `{}` if enumeration fails).

#### `os.platform()`

- **Added:** v0.5.0
- **Params:** none.
- **Returns:** `string` — one of `'aix'`, `'darwin'`, `'freebsd'`, `'linux'`, `'openbsd'`, `'sunos'`, `'win32'`, `'android'` (experimental).
- **Throws:** never. Equivalent to `process.platform`.

#### `os.release()`

- **Added:** v0.3.3
- **Params:** none.
- **Returns:** `string` — OS release/version string (e.g. Linux kernel version, Windows build number, Darwin version).
- **Throws:** never.

#### `os.setPriority([pid, ]priority)`

- **Added:** v10.10.0
- **Params:**

  | Name | Type | Optional | Default |
  |---|---|---|---|
  | `pid` | `integer` | yes | `0` (meaning the calling process) |
  | `priority` | `integer` | no | — (range `-20` to `19`; clamped into this range, then mapped to the nearest `PRIORITY_*` bucket) |

- **Returns:** `void`.
- **Throws:** `TypeError` (`ERR_INVALID_ARG_TYPE`/`ERR_OUT_OF_RANGE`) for bad argument shape/range; `SystemError` (`ESRCH` unknown pid, `EACCES`/`EPERM` insufficient privilege — notably raising to `PRIORITY_HIGHEST` on Windows requires elevated privileges).

#### `os.tmpdir()`

- **Added:** v0.9.9
- **Params:** none.
- **Returns:** `string` — default directory for temporary files, with no trailing slash.
- **Throws:** never.
- **Windows:** `TEMP`, then `TMP`, else `%SystemRoot%\temp`.
- **POSIX:** `TMPDIR`, then `TMP`, then `TEMP`, else `/tmp`.

#### `os.totalmem()`

- **Added:** v0.3.3
- **Params:** none.
- **Returns:** `integer` — total system memory in bytes.
- **Throws:** never.

#### `os.type()`

- **Added:** v0.3.3
- **Params:** none.
- **Returns:** `string` — `uname(3)`-style OS name, e.g. `'Linux'`, `'Darwin'`, `'Windows_NT'`.
- **Throws:** never.

#### `os.uptime()`

- **Added:** v0.3.3
- **Params:** none.
- **Returns:** `number` — system uptime in seconds (may carry a fractional part depending on platform clock resolution).
- **Throws:** never.

#### `os.userInfo([options])`

- **Added:** v6.0.0
- **Params:**

  | Name | Type | Optional | Default |
  |---|---|---|---|
  | `options` | `{ encoding?: string }` | yes | `{ encoding: 'utf8' }` |

- **Returns:** `UserInfo<string>` normally, or `UserInfo<Buffer>` when `options.encoding === 'buffer'`.
- **Throws:** `SystemError` if the current user has no `username` or `homedir` resolvable on the host.

#### `os.version()`

- **Added:** v13.11.0, v12.17.0
- **Params:** none.
- **Returns:** `string` — a human-readable kernel/OS build identifier (e.g. `uname -v` output on POSIX, Windows build string on Windows).
- **Throws:** never.

### Properties & constants (`os.constants`)

#### Signal constants (`os.constants.signals`) — all `number`

| Constant | Description |
|---|---|
| `SIGHUP` | Controlling terminal closed or parent process exited |
| `SIGINT` | User interrupt (Ctrl+C) |
| `SIGQUIT` | User terminate with core dump |
| `SIGILL` | Illegal/malformed/unknown/privileged instruction |
| `SIGTRAP` | Exception occurred |
| `SIGABRT` | Request process abort |
| `SIGIOT` | Synonym for `SIGABRT` |
| `SIGBUS` | Bus error |
| `SIGFPE` | Illegal arithmetic operation |
| `SIGKILL` | Immediate termination |
| `SIGUSR1` | User-defined condition 1 |
| `SIGUSR2` | User-defined condition 2 |
| `SIGSEGV` | Segmentation fault |
| `SIGPIPE` | Write to disconnected pipe |
| `SIGALRM` | System timer elapsed |
| `SIGTERM` | Termination request |
| `SIGCHLD` | Child process terminated |
| `SIGSTKFLT` | Stack fault on coprocessor |
| `SIGCONT` | Continue paused process |
| `SIGSTOP` | Halt process |
| `SIGTSTP` | Stop request |
| `SIGBREAK` | User interrupt request (Windows) |
| `SIGTTIN` | Read from TTY in background |
| `SIGTTOU` | Write to TTY in background |
| `SIGURG` | Urgent data to read on socket |
| `SIGXCPU` | CPU usage limit exceeded |
| `SIGXFSZ` | File size exceeds maximum |
| `SIGVTALRM` | Virtual timer elapsed |
| `SIGPROF` | Profiling timer elapsed |
| `SIGWINCH` | Controlling terminal size changed |
| `SIGIO` | I/O available |
| `SIGPOLL` | Synonym for `SIGIO` |
| `SIGLOST` | File lock lost |
| `SIGPWR` | Power failure notification |
| `SIGINFO` | Synonym for `SIGPWR` |
| `SIGSYS` | Bad argument notification |
| `SIGUNUSED` | Synonym for `SIGSYS` |

Availability/exact numeric value is platform-dependent; not every signal exists on every OS (e.g. `SIGBREAK` is Windows-only, `SIGINFO` mostly BSD/Darwin). Missing signals on a given platform must be omitted (never fabricated), matching Node's own behavior.

#### Error constants (`os.constants.errno`) — all `number`

POSIX-family (present on all platforms, values are the errno-header value for the host libc):

| Constant | Description |
|---|---|
| `E2BIG` | Argument list too long |
| `EACCES` | Insufficient permissions |
| `EADDRINUSE` | Network address already in use |
| `EADDRNOTAVAIL` | Network address unavailable |
| `EAFNOSUPPORT` | Network address family not supported |
| `EAGAIN` | No data available, try again later |
| `EALREADY` | Socket has pending connection in progress |
| `EBADF` | Invalid file descriptor |
| `EBADMSG` | Invalid data message |
| `EBUSY` | Device or resource busy |
| `ECANCELED` | Operation canceled |
| `ECHILD` | No child processes |
| `ECONNABORTED` | Network connection aborted |
| `ECONNREFUSED` | Network connection refused |
| `ECONNRESET` | Network connection reset |
| `EDEADLK` | Resource deadlock avoided |
| `EDESTADDRREQ` | Destination address required |
| `EDOM` | Argument out of function domain |
| `EDQUOT` | Disk quota exceeded |
| `EEXIST` | File already exists |
| `EFAULT` | Invalid pointer address |
| `EFBIG` | File too large |
| `EHOSTUNREACH` | Host unreachable |
| `EIDRM` | Identifier removed |
| `EILSEQ` | Illegal byte sequence |
| `EINPROGRESS` | Operation already in progress |
| `EINTR` | Function call interrupted |
| `EINVAL` | Invalid argument |
| `EIO` | Unspecified I/O error |
| `EISCONN` | Socket is connected |
| `EISDIR` | Path is a directory |
| `ELOOP` | Too many symbolic links in path |
| `EMFILE` | Too many open files |
| `EMLINK` | Too many hard links to file |
| `EMSGSIZE` | Message too long |
| `EMULTIHOP` | Multihop attempted |
| `ENAMETOOLONG` | Filename too long |
| `ENETDOWN` | Network is down |
| `ENETRESET` | Connection aborted by network |
| `ENETUNREACH` | Network unreachable |
| `ENFILE` | Too many open files in system |
| `ENOBUFS` | No buffer space available |
| `ENODATA` | No message on stream head read queue |
| `ENODEV` | No such device |
| `ENOENT` | No such file or directory |
| `ENOEXEC` | Exec format error |
| `ENOLCK` | No locks available |
| `ENOLINK` | Link severed |
| `ENOMEM` | Not enough space |
| `ENOMSG` | No message of desired type |
| `ENOPROTOOPT` | Protocol not available |
| `ENOSPC` | No space on device |
| `ENOSR` | No stream resources available |
| `ENOSTR` | Resource not a stream |
| `ENOSYS` | Function not implemented |
| `ENOTCONN` | Socket not connected |
| `ENOTDIR` | Path is not a directory |
| `ENOTEMPTY` | Directory not empty |
| `ENOTSOCK` | Item is not a socket |
| `ENOTSUP` | Operation not supported |
| `ENOTTY` | Inappropriate I/O control operation |
| `ENXIO` | No such device or address |
| `EOPNOTSUPP` | Operation not supported on socket |
| `EOVERFLOW` | Value too large for data type |
| `EPERM` | Operation not permitted |
| `EPIPE` | Broken pipe |
| `EPROTO` | Protocol error |
| `EPROTONOSUPPORT` | Protocol not supported |
| `EPROTOTYPE` | Wrong protocol type for socket |
| `ERANGE` | Results too large |
| `EROFS` | File system is read-only |
| `ESPIPE` | Invalid seek operation |
| `ESRCH` | No such process |
| `ESTALE` | Stale file handle |
| `ETIME` | Timer expired |
| `ETIMEDOUT` | Connection timed out |
| `ETXTBSY` | Text file is busy |
| `EWOULDBLOCK` | Operation would block |
| `EXDEV` | Improper link |

Windows-specific (`WSA*`, only meaningfully populated when the RTS binary is compiled for the `win32` target):

| Constant | Description |
|---|---|
| `WSAEINTR` | Interrupted function call |
| `WSAEBADF` | Invalid file handle |
| `WSAEACCES` | Insufficient permissions |
| `WSAEFAULT` | Invalid pointer address |
| `WSAEINVAL` | Invalid argument |
| `WSAEMFILE` | Too many open files |
| `WSAEWOULDBLOCK` | Resource temporarily unavailable |
| `WSAEINPROGRESS` | Operation in progress |
| `WSAEALREADY` | Operation already in progress |
| `WSAENOTSOCK` | Resource is not a socket |
| `WSAEDESTADDRREQ` | Destination address required |
| `WSAEMSGSIZE` | Message size too long |
| `WSAEPROTOTYPE` | Wrong protocol type for socket |
| `WSAENOPROTOOPT` | Bad protocol option |
| `WSAEPROTONOSUPPORT` | Protocol not supported |
| `WSAESOCKTNOSUPPORT` | Socket type not supported |
| `WSAEOPNOTSUPP` | Operation not supported |
| `WSAEPFNOSUPPORT` | Protocol family not supported |
| `WSAEAFNOSUPPORT` | Address family not supported |
| `WSAEADDRINUSE` | Network address already in use |
| `WSAEADDRNOTAVAIL` | Network address not available |
| `WSAENETDOWN` | Network is down |
| `WSAENETUNREACH` | Network unreachable |
| `WSAENETRESET` | Network connection reset |
| `WSAECONNABORTED` | Connection aborted |
| `WSAECONNRESET` | Connection reset by peer |
| `WSAENOBUFS` | No buffer space available |
| `WSAEISCONN` | Socket already connected |
| `WSAENOTCONN` | Socket not connected |
| `WSAESHUTDOWN` | Cannot send data after shutdown |
| `WSAETOOMANYREFS` | Too many references |
| `WSAETIMEDOUT` | Connection timed out |
| `WSAECONNREFUSED` | Connection refused |
| `WSAELOOP` | Name cannot be translated |
| `WSAENAMETOOLONG` | Name too long |
| `WSAEHOSTDOWN` | Network host is down |
| `WSAEHOSTUNREACH` | No route to network host |
| `WSAENOTEMPTY` | Directory not empty |
| `WSAEPROCLIM` | Too many processes |
| `WSAEUSERS` | User quota exceeded |
| `WSAEDQUOT` | Disk quota exceeded |
| `WSAESTALE` | Stale file handle |
| `WSAEREMOTE` | Item is remote |
| `WSASYSNOTREADY` | Network subsystem not ready |
| `WSAVERNOTSUPPORTED` | `winsock.dll` version out of range |
| `WSANOTINITIALISED` | WSAStartup not yet performed |
| `WSAEDISCON` | Graceful shutdown in progress |
| `WSAENOMORE` | No more results |
| `WSAECANCELLED` | Operation canceled |
| `WSAEINVALIDPROCTABLE` | Procedure call table invalid |
| `WSAEINVALIDPROVIDER` | Invalid service provider |
| `WSAEPROVIDERFAILEDINIT` | Service provider failed to initialize |
| `WSASYSCALLFAILURE` | System call failure |
| `WSASERVICE_NOT_FOUND` | Service not found |
| `WSATYPE_NOT_FOUND` | Class type not found |
| `WSA_E_NO_MORE` | No more results |
| `WSA_E_CANCELLED` | Call canceled |
| `WSAEREFUSED` | Database query refused |

#### `dlopen` constants (`os.constants.dlopen`) — all `number`

| Constant | Description |
|---|---|
| `RTLD_LAZY` | Perform lazy binding (Node.js default) |
| `RTLD_NOW` | Resolve all undefined symbols before `dlopen(3)` returns |
| `RTLD_GLOBAL` | Symbols available to subsequently loaded libraries |
| `RTLD_LOCAL` | Opposite of `RTLD_GLOBAL`; default if neither specified |
| `RTLD_DEEPBIND` | Self-contained library prefers its own symbols |

Populated on POSIX; on Windows this object is empty (no `dlopen(3)` concept), matching Node.

#### Priority constants (`os.constants.priority`) — all `number`, `-20..19` range

| Constant | Value | Description |
|---|---|---|
| `PRIORITY_LOW` | `19` | Lowest priority (`IDLE_PRIORITY_CLASS` on Windows; nice `19` elsewhere) |
| `PRIORITY_BELOW_NORMAL` | `10` | Above `PRIORITY_LOW`, below `PRIORITY_NORMAL` (`BELOW_NORMAL_PRIORITY_CLASS` on Windows; nice `10` elsewhere) |
| `PRIORITY_NORMAL` | `0` | Default priority (`NORMAL_PRIORITY_CLASS` on Windows; nice `0` elsewhere) |
| `PRIORITY_ABOVE_NORMAL` | `-7` | Above `PRIORITY_NORMAL`, below `PRIORITY_HIGH` (`ABOVE_NORMAL_PRIORITY_CLASS` on Windows; nice `-7` elsewhere) |
| `PRIORITY_HIGH` | `-14` | Above `PRIORITY_ABOVE_NORMAL`, below `PRIORITY_HIGHEST` (`HIGH_PRIORITY_CLASS` on Windows; nice `-14` elsewhere) |
| `PRIORITY_HIGHEST` | `-20` | Highest priority (`REALTIME_PRIORITY_CLASS` on Windows; nice `-20` elsewhere) |

#### libuv constants (`os.constants.libuv`) — all `number`

| Constant | Description |
|---|---|
| `UV_UDP_REUSEADDR` | Flag value libuv uses internally for UDP socket `SO_REUSEADDR` behavior; exposed on `os.constants` purely as a mirror of the libuv build, not something `node:os` itself acts on (the actual UDP behavior lives in `node:dgram`/`node:net`). |

### Events

None. `node:os` has no `EventEmitter`-derived object anywhere in its surface.

## 3. Types & option objects

```typescript
interface CpuTimes {
  user: number; // milliseconds spent in user mode
  nice: number; // milliseconds spent in nice mode (always 0 on Windows; POSIX only)
  sys: number;  // milliseconds spent in sys (kernel) mode
  idle: number; // milliseconds spent idle
  irq: number;  // milliseconds spent servicing interrupts
}

interface CpuInfo {
  model: string;   // e.g. "Intel(R) Core(TM) i7-9750H CPU @ 2.60GHz"
  speed: number;    // clock speed in MHz (0 if undeterminable)
  times: CpuTimes;
}

interface NetworkInterfaceInfoV4 {
  address: string;      // e.g. "192.168.1.2"
  netmask: string;       // e.g. "255.255.255.0"
  family: 'IPv4';
  mac: string;           // e.g. "00:1a:2b:3c:4d:5e"; "00:00:00:00:00:00" if unavailable
  internal: boolean;     // true for loopback/non-remote-reachable interfaces
  cidr: string | null;   // e.g. "192.168.1.2/24"; null if netmask malformed
}

interface NetworkInterfaceInfoV6 {
  address: string;       // e.g. "fe80::1"
  netmask: string;
  family: 'IPv6';
  mac: string;
  internal: boolean;
  scopeid: number;       // numeric IPv6 scope id (0 for non-link-local)
  cidr: string | null;    // e.g. "fe80::1/64"
}

type NetworkInterfaceInfo = NetworkInterfaceInfoV4 | NetworkInterfaceInfoV6;

type NetworkInterfaces = Record<string, NetworkInterfaceInfo[]>;

interface UserInfoOptions {
  encoding?: BufferEncoding | 'buffer'; // default 'utf8'
}

interface UserInfo<T = string> {
  username: T;
  uid: number;   // -1 on Windows
  gid: number;   // -1 on Windows
  shell: T | null; // null on Windows
  homedir: T;
}

interface OsPriorityConstants {
  PRIORITY_LOW: number;
  PRIORITY_BELOW_NORMAL: number;
  PRIORITY_NORMAL: number;
  PRIORITY_ABOVE_NORMAL: number;
  PRIORITY_HIGH: number;
  PRIORITY_HIGHEST: number;
}

interface OsDlopenConstants {
  RTLD_LAZY?: number;
  RTLD_NOW?: number;
  RTLD_GLOBAL?: number;
  RTLD_LOCAL?: number;
  RTLD_DEEPBIND?: number;
}

interface OsConstants {
  signals: Record<string, number>;   // subset of NodeJS.Signals present on this platform
  errno: Record<string, number>;      // POSIX E* on all platforms + WSA* additionally on win32
  dlopen: OsDlopenConstants;           // empty object on win32
  priority: OsPriorityConstants;
  libuv: { UV_UDP_REUSEADDR: number };
}
```

No callback signatures exist in this module (every function is synchronous); the only "shape" contracts are the return types above.

## 4. Node semantics & edge cases

- **Everything is synchronous.** No function in `node:os` accepts a callback or returns a `Promise`; there is no `node:os/promises` sibling module (unlike `fs`/`dns`/`timers`).
- **`os.availableParallelism()` vs `os.cpus().length`.** These answer different questions: `cpus().length` always lists every logical core the OS reports, while `availableParallelism()` (added specifically to fix container/cgroup-unaware code) accounts for CPU affinity masks and cgroup v1/v2 CPU quota — inside a container limited to 2 CPUs on a 32-core host, `cpus().length` returns `32` but `availableParallelism()` returns `2`. RTS must not implement `availableParallelism()` as a thin alias of `cpus().length`.
- **`os.arch()`/`os.platform()` are compiled-in, not detected.** They mirror `process.arch`/`process.platform` — the values are fixed by the target the RTS binary was built for, not probed at runtime (important for AOT cross-compiled binaries: a binary built for `--target linux-x64` reports `'linux'`/`'x64'` even if run inside an emulation layer that presents differently to `uname`).
- **`os.machine()` is the raw `uname -m` (or equivalent) value**, a materially different enum from `os.arch()` — e.g. `os.arch()` returns `'x64'` while `os.machine()` returns `'x86_64'`; `os.arch()` returns `'arm64'` while `os.machine()` may return `'aarch64'` or `'arm64'` depending on OS.
- **`os.loadavg()` is `[0, 0, 0]` on Windows** unconditionally — there is no Windows equivalent of the Unix load-average concept; RTS must not attempt to synthesize one from CPU usage samples (that would not be parity, it would be a different metric).
- **`os.userInfo()` Windows fields.** `uid`/`gid` are always `-1` and `shell` is always `null` on Windows (no POSIX shell/uid/gid concept); on POSIX these are real values from the password database. `os.userInfo({ encoding: 'buffer' })` returns `username`/`shell`/`homedir` as `Buffer` instances instead of `string` (raw OS bytes, useful when the OS-level encoding isn't valid UTF-8).
- **`os.getPriority`/`os.setPriority` mapping.** Node exposes a single `-20..19` (Unix `nice`-style) integer range on **every** platform, including Windows, by mapping onto the 6 `PRIORITY_*` buckets — Windows priority classes only have 6 discrete levels, so any requested integer is bucketed into the nearest `PRIORITY_*` constant, not applied as a literal nice value. `setPriority` to `PRIORITY_HIGHEST`/near-realtime requires elevated privileges on Windows and typically `CAP_SYS_NICE` (or running as root) on Linux for negative nice values; insufficient privilege surfaces as `EACCES`/`EPERM`. A `pid` that doesn't exist surfaces `ESRCH`.
- **`pid: 0` means the calling process** for both `getPriority`/`setPriority` (standard `nice(2)`/`getpriority(2)` convention), not "no pid"/"all processes".
- **`os.tmpdir()`/`os.homedir()` env-var precedence** differs by platform (see section 2 above); RTS should honor the exact same variable-name precedence order Node documents, not just "whatever `std::env::temp_dir()` returns" (Rust's own default may differ in precedence order from Node's, particularly around `TMPDIR` vs `TMP` vs `TEMP` ordering on POSIX).
- **`os.networkInterfaces()` ordering and `cidr: null` fallback.** Interface iteration order is OS-dependent (not alphabetical or otherwise guaranteed); if computing CIDR notation from address+netmask fails (malformed netmask), `cidr` must be `null` rather than throwing or omitting the field. Loopback interfaces (`'lo'` on Linux, `'lo0'` on macOS, `'Loopback Pseudo-Interface 1'` on Windows) must have `internal: true`; MAC address is `'00:00:00:00:00:00'` when not applicable (e.g. loopback).
- **Signal/errno constant availability is platform-conditional**, not just platform-valued — e.g. `SIGBREAK` literally does not exist as a key on POSIX platforms' `os.constants.signals` (not merely "0" or "undefined value", the key itself is absent), and the `WSA*` errno family only exists on `win32`. RTS's constants table must omit rather than zero-fill unavailable entries, matching Node.
- **No deprecated members remain** in `node:os` as of Node 25 (the old `os.tmpDir()` capitalized alias was removed in Node 7 and is not part of this parity target).
- **No backpressure/ordering concerns** — nothing in this module streams data or has multi-step ordering semantics; every call is a single bounded query.
- **Security note.** `os.userInfo()`, `os.hostname()`, and `os.networkInterfaces()` (MAC addresses, internal IPs) leak host-identifying information to any script that can call them; RTS applies no additional sandboxing here beyond what Node itself does (none) — any future RTS permission-model gating of `node:os` is a policy decision orthogonal to this parity spec.

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node` is a fully independent crate (no `rts-std` dependency) and owns its own OS-facing implementation for every group below. This module is unusual among `node:*` modules in requiring **zero async infrastructure** — every function is a direct, bounded syscall wrapper.

- **Identity strings (`arch`/`platform`/`type`/`release`/`version`/`machine`/`endianness`).** `arch`/`platform`/`endianness` derive from `std::env::consts::ARCH`/`std::env::consts::OS` plus a small normalization table (Node's `arch`/`platform` enums do not match Rust's `std::env::consts::ARCH`/`OS` strings 1:1 — e.g. Rust's `"macos"` → Node's `"darwin"`, Rust's `"windows"` → Node's `"win32"`). `type`/`release`/`version`/`machine` need the actual `uname(3)` struct on POSIX (`libc::uname`) and `RtlGetVersion`/registry `CurrentBuildNumber` + `GetNativeSystemInfo` on Windows (`windows-sys` crate) — these cannot come from compile-time constants because they report the *running kernel's* version, not the compiled target.
- **Memory (`freemem`/`totalmem`).** Linux: `libc::sysconf(_SC_PHYS_PAGES) * sysconf(_SC_PAGESIZE)` for total, `/proc/meminfo` `MemAvailable` (preferred) or `sysinfo(2)` for free. macOS: `sysctlbyname("hw.memsize")` for total, `host_statistics64(HOST_VM_INFO64)` for free. Windows: `GlobalMemoryStatusEx`.
- **Load average (`loadavg`).** POSIX: `libc::getloadavg(3)`. Windows: hardcoded `[0.0, 0.0, 0.0]` (no OS concept, per Node semantics above — must not be synthesized).
- **Uptime (`uptime`).** Linux: `/proc/uptime` (first field) or `sysinfo(2)`. macOS: `sysctl(KERN_BOOTTIME)` diffed against current time. Windows: `GetTickCount64()` (ms → seconds).
- **CPU topology (`cpus`).** The highest-effort area. Linux: parse `/proc/cpuinfo` for `model name`/`cpu MHz` per core, `/proc/stat` `cpu0`, `cpu1`, … lines for the `user/nice/sys/idle/irq` jiffie counters (convert `USER_HZ` ticks → ms via `sysconf(_SC_CLK_TCK)`). macOS: `sysctlbyname("machdep.cpu.brand_string")` + `host_processor_info(PROCESSOR_CPU_LOAD_INFO)` per core. Windows: `GetLogicalProcessorInformationEx` for topology + `NtQuerySystemInformation(SystemProcessorPerformanceInformation)` for per-core tick counts, model name from the registry (`HARDWARE\DESCRIPTION\System\CentralProcessor\%d`).
- **`availableParallelism`.** `std::thread::available_parallelism()` as the base (respects OS-level affinity mask on all three platforms via the underlying `sched_getaffinity`/`GetProcessAffinityMask`/`pthread_getaffinity_np`), **plus** cgroup v1/v2 CPU quota parsing on Linux (`/sys/fs/cgroup/cpu.max` v2, or `cpu.cfs_quota_us`/`cpu.cfs_period_us` v1) to match Node's container-aware behavior — this is the one place `std::thread::available_parallelism()` alone is insufficient for full parity (see open questions).
- **`networkInterfaces`.** POSIX: `libc::getifaddrs(3)` walking the linked list, computing CIDR from netmask bit-count. Windows: `GetAdaptersAddresses`. Alternatively the pure-Rust `if-addrs` crate wraps both platforms' enumeration without extra FFI surface — acceptable since it has no async/tokio footprint and matches the "owns its own native impl, small focused crates OK" model already used for e.g. `flate2`/`rustls` elsewhere in `rts-node`'s sibling modules.
- **`homedir`/`tmpdir`/`hostname`.** `homedir`: `$HOME` env / POSIX `getpwuid_r(3)` fallback; Windows `USERPROFILE` env / `SHGetKnownFolderPath(FOLDERID_Profile)` fallback. `tmpdir`: manual env-var precedence chain per platform (not `std::env::temp_dir()`, whose precedence doesn't documented-match Node's exactly — verify before relying on it). `hostname`: POSIX `libc::gethostname(2)`; Windows `GetComputerNameExW(ComputerNamePhysicalDnsHostname)`.
- **`userInfo`.** POSIX: `libc::geteuid()` + `getpwuid_r(3)` for username/homedir/shell/gid. Windows: `GetUserNameW` for username, `USERPROFILE`/known-folder for homedir, `uid`/`gid` hardcoded `-1`, `shell` hardcoded `null`.
- **`getPriority`/`setPriority`.** POSIX: `libc::getpriority(2)`/`setpriority(2)` with `PRIO_PROCESS`. Windows: `GetPriorityClass`/`SetPriorityClass` via `OpenProcess`, mapped through the 6-bucket `PRIORITY_*` table (nearest-bucket rounding for arbitrary input integers, matching Node's own libuv-level mapping).
- **`os.constants`.** A single Rust `const`/static table assembled with `#[cfg(target_os = ...)]`/`#[cfg(unix)]`/`#[cfg(windows)]` gates so each compiled target embeds exactly the signal/errno/dlopen entries that exist on it (sourced from the `libc` crate's platform-specific constant values, plus Node's own fixed `priority`/`libuv` values which are platform-independent Node-level conventions, not OS-sourced).
- **`EOL`/`devNull`.** Pure compile-time literals (`cfg!(windows)` branch) — no runtime syscall needed at all; can be resolved either as a trivial native call or as a `.ts`-side literal keyed off `os.platform()` (implementation choice, see 5.2).

### 5.2 ABI surface

Symbol convention: `__RTS_FN_NODE_OS_<NAME>`. This module needs **no `Handle`-typed values at all** — every function returns either a primitive (`StrPtr`/`I32`/`I64`/`U64`/`F64`) or a small/variable-shaped compound result, which is carried as a single JSON-encoded `StrPtr` decoded by a thin `.ts` shim (the same pattern used for compound/heterogeneous results in other `rts-node` modules) rather than inventing bespoke multi-slot ABI shapes for `CpuInfo[]`/`NetworkInterfaceInfo[]`/`UserInfo`/the constants table.

| Symbol | Args (AbiType) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_OS_ARCH` | (none) | `StrPtr` | Node-normalized arch enum |
| `__RTS_FN_NODE_OS_PLATFORM` | (none) | `StrPtr` | Node-normalized platform enum |
| `__RTS_FN_NODE_OS_TYPE` | (none) | `StrPtr` | `uname -s`-equivalent |
| `__RTS_FN_NODE_OS_RELEASE` | (none) | `StrPtr` | `uname -r`-equivalent |
| `__RTS_FN_NODE_OS_VERSION` | (none) | `StrPtr` | `uname -v`-equivalent / Windows build string |
| `__RTS_FN_NODE_OS_MACHINE` | (none) | `StrPtr` | raw `uname -m`-equivalent |
| `__RTS_FN_NODE_OS_ENDIANNESS` | (none) | `StrPtr` | `"BE"` \| `"LE"` |
| `__RTS_FN_NODE_OS_HOSTNAME` | (none) | `StrPtr` | |
| `__RTS_FN_NODE_OS_HOMEDIR` | (none) | `StrPtr` | |
| `__RTS_FN_NODE_OS_TMPDIR` | (none) | `StrPtr` | no trailing slash |
| `__RTS_FN_NODE_OS_EOL` | (none) | `StrPtr` | trivial; could alternatively be a pure `.ts` literal (implementation choice) |
| `__RTS_FN_NODE_OS_DEV_NULL` | (none) | `StrPtr` | ditto |
| `__RTS_FN_NODE_OS_FREEMEM` | (none) | `U64` | bytes |
| `__RTS_FN_NODE_OS_TOTALMEM` | (none) | `U64` | bytes |
| `__RTS_FN_NODE_OS_UPTIME` | (none) | `F64` | seconds, may be fractional |
| `__RTS_FN_NODE_OS_LOADAVG_1` | (none) | `F64` | 1-minute load average |
| `__RTS_FN_NODE_OS_LOADAVG_5` | (none) | `F64` | 5-minute load average |
| `__RTS_FN_NODE_OS_LOADAVG_15` | (none) | `F64` | 15-minute load average |
| `__RTS_FN_NODE_OS_AVAILABLE_PARALLELISM` | (none) | `U64` | cgroup/affinity-aware |
| `__RTS_FN_NODE_OS_CPUS_JSON` | (none) | `StrPtr` | JSON array of `CpuInfo` |
| `__RTS_FN_NODE_OS_NETWORK_INTERFACES_JSON` | (none) | `StrPtr` | JSON `Record<string, NetworkInterfaceInfo[]>` |
| `__RTS_FN_NODE_OS_USER_INFO_JSON` | `Bool asBuffer` | `StrPtr` | JSON `UserInfo`; when `asBuffer`, string fields are base64-tagged for `.ts` to reinflate as `Buffer` |
| `__RTS_FN_NODE_OS_GET_PRIORITY` | `I32 pid` | `I32` | throws via thread-local error slot (`ESRCH`/`EPERM`) |
| `__RTS_FN_NODE_OS_SET_PRIORITY` | `I32 pid, I32 priority` | `Void` | throws via thread-local error slot |
| `__RTS_FN_NODE_OS_CONSTANTS_JSON` | (none) | `StrPtr` | one JSON blob: `{signals, errno, dlopen, priority, libuv}`, assembled at Rust compile time per target so cross-compiled binaries embed the correct per-target constant set |

Why `LOADAVG_1`/`_5`/`_15` as three scalar `F64` getters instead of one JSON blob: three numbers is exactly the case where a JSON round-trip is pure overhead — three direct scalar externs are simpler and the `.ts` shim just builds `[loadavg1(), loadavg5(), loadavg15()]`. Everything else that is genuinely variable-shaped (`cpus`, `networkInterfaces`, `userInfo`, `constants`) uses the JSON-blob pattern, consistent with how other `rts-node` modules handle heterogeneous/array results without inventing a bespoke ABI shape per return type.

### 5.3 Async model

**None needed.** Every function in `node:os` is synchronous in Node itself and must remain synchronous in RTS — there is no callback variant, no promise variant, and no `node:os/promises`. Native calls execute directly on the calling thread and return immediately (all underlying syscalls are non-blocking-in-practice: reading `/proc` files, `sysctl`, `uname`, `getifaddrs`, in-memory OS tables). No tokio runtime, no promise-subsystem `create`/`wait`, no event-loop interaction of any kind is required for this module. This makes `node:os` a good **first module to implement** in the rts-node rewrite, since it validates the `NodespaceSpec`/ABI/`.ts`-shim plumbing without needing the async infra hoist that most other `node:*` modules require (contrast with `node:dns`, `node:fs/promises`, etc.).

### 5.4 Multithread / worker interaction

- Nearly every function in this module is a **stateless, side-effect-free query** against host-global OS state (CPU info, memory, hostname, network interfaces, user identity) — safe to call concurrently from any number of RTS threads/workers with no locking, no `threadLocal` region, and no shared-heap promotion concerns. Two RTS threads calling `os.cpus()` simultaneously simply each perform their own independent read of `/proc`/`sysctl`/Windows APIs; there is no module-owned mutable state to race on.
- The one exception is **`os.setPriority(pid, priority)`**: scheduling priority is an OS **process**-level (or, for `pid` referring to a specific thread ID on some platforms, thread-level) attribute, not something owned by the calling RTS logical thread/region. When `pid` is `0` (the common case, "the calling process"), `setPriority` mutates the priority of the **entire OS process**, which affects every RTS worker thread inside it — this must be documented as a process-wide side effect regardless of which RTS thread/region invoked the call, and it is **not** appropriate to model as `threadLocal` state under `docs/specs/rts-threading-model.md`; it is closer to `shared` (process-global) mutable state, but mutated via direct OS syscall rather than the RTS shared-heap.
- No `SharedArrayBuffer`/channel/`MessagePort` semantics apply anywhere in this module — nothing here is a transferable object, and nothing needs `worker_threads` special-casing beyond the `setPriority` note above.
- `os.availableParallelism()`'s cgroup-affinity-aware answer can legitimately differ if RTS itself spawns OS threads that get pinned/affinitized differently mid-process; RTS should query fresh each call (no caching) to stay accurate, matching Node's own no-caching behavior.

### 5.5 Buffer / TypedArray interop

The only Buffer-shaped surface in this module is **`os.userInfo({ encoding: 'buffer' })`**: `username`, `shell`, and `homedir` become `Buffer` instances (raw OS-encoding bytes) instead of UTF-8-decoded `string`s. Native implementation: `__RTS_FN_NODE_OS_USER_INFO_JSON(asBuffer: Bool)` always returns the raw bytes base64-encoded inside the JSON payload when `asBuffer` is true (or plain UTF-8 strings when false); the `.ts` shim base64-decodes into `Buffer.from(..., 'base64')` (`Buffer extends Uint8Array` per the engine's primordial TypedArray model) only in the `asBuffer` branch. No other function in this module touches binary data — `mac` addresses and `cidr` strings in `networkInterfaces()` are always plain display strings, never raw bytes.

### 5.6 Doctrine placement

`node:os` is **non-primordial** — the engine (`rts-codegen-new`) must never hardcode `"os"` or any of its member names. Resolution follows the existing `NodespaceSpec` mechanism already implemented in `crates/rts-node/src/lib.rs`: `import os from 'node:os'` is mapped through `rts_node::ns_prefix_for("node:os")` → `"node_os"` (pure data lookup against `NODE_SPECS`, no hardcoded arm in codegen), and each call like `node_os.cpus()` resolves via `rts_node::node_lookup("node_os.cpus")` to a `NodespaceMember` (`symbol`, `args`, `returns`) — the same generic path every other `node:*` module uses. `crates/rts-node/src/os/mod.rs` currently exists as a **thin table borrowing `__RTS_FN_NS_OS_*` symbols from `rts-std`** (4 members: `platform`/`arch`/`homedir`/`tmpdir`) — per the owner decision this is deleted and replaced by the native-owned `__RTS_FN_NODE_OS_*` symbols in 5.2, with the full member surface from section 2 (not just the 4 legacy ones).

Native-extern / `.ts`-shim split: every symbol in 5.2 is a raw primitive (scalar getter or JSON-blob getter; two with args for priority get/set). All JS-shape ergonomics — the `UserInfo`/`CpuInfo`/`NetworkInterfaceInfo` object construction from decoded JSON, the `os.constants` frozen nested-object assembly (parsed once from `CONSTANTS_JSON` and cached), the `loadavg()` 3-element array assembly from the three scalar getters, and the `encoding: 'buffer'` branch of `userInfo` — live in a `.ts` shim shipped by `rts-node` (e.g. `rts-node/src/os/os.ts`).

### 5.7 Shared-infra dependencies (FLAG)

None. `node:os` needs no promise/settle subsystem, no shared tokio runtime, no GC thread-registry hook, and no `HandleTable` slab — every function is a direct synchronous syscall wrapper returning a primitive or a JSON `StrPtr` blob. This makes it one of the few `node:*` modules with **zero hoist prerequisites** from `rts-std`, and a good first target to de-risk the rewritten `rts-node` → `NODE_SPECS` → engine-Registry plumbing before tackling modules that do need the async-infra hoist (`node:dns`, `node:net`, `node:fs/promises`, `node:worker_threads`, etc.).

### 5.8 Implementation phases

1. **(a)** Replace `crates/rts-node/src/os/mod.rs`'s current 4-member thin table (borrowed `__RTS_FN_NS_OS_*` symbols) with the full `NodespaceSpec` skeleton for all 20 functions + `EOL`/`devNull`/`constants` from section 2, still registered as `node_module: "os"`, `ns_prefix: "node_os"` in `NODE_SPECS`.
2. **(b)** Implement the identity-string group (`arch`/`platform`/`type`/`release`/`version`/`machine`/`endianness`/`EOL`/`devNull`) — no external crate needed beyond `libc`/`windows-sys` for `uname`/`RtlGetVersion`; establishes the `StrPtr`-returning native-fn pattern for this module.
3. **(c)** Implement the memory/load group (`freemem`/`totalmem`/`loadavg` ×3/`uptime`/`availableParallelism`) — `std::thread::available_parallelism()` first, cgroup-quota refinement as a follow-up sub-step (see open questions).
4. **(d)** Implement `hostname`/`homedir`/`tmpdir` with the exact per-platform env-var precedence chains from section 4 (do not rely on `std::env::temp_dir()`'s built-in precedence without verifying it matches Node's).
5. **(e)** Implement `os.constants` as the compile-time `cfg`-gated data table + single `CONSTANTS_JSON` extern; wire the `.ts` shim to parse-once-and-freeze.
6. **(f)** Implement `networkInterfaces()` (via `if-addrs` crate or direct `getifaddrs`/`GetAdaptersAddresses` FFI) and `userInfo()` (including the `encoding: 'buffer'` branch) — these are the two JSON-blob-with-nontrivial-shape results.
7. **(g)** Implement `cpus()` — the highest-effort item (per-platform `/proc` parsing, `sysctl`, or `NtQuerySystemInformation`); can land after (f) since nothing else depends on it.
8. **(h)** Implement `getPriority`/`setPriority` with the 6-bucket nearest-priority-class mapping table (shared between POSIX nice-value passthrough and Windows priority-class bucketing).
9. **(i)** Cross-platform verification pass: build and run the full test plan (section 6) on Windows, Linux, and macOS CI legs, paying special attention to the `constants` key-presence differences and the `loadavg`/`userInfo` Windows-specific values.

## 6. Test plan

```
tests/node/os/os_identity.test.ts
  - os.arch() is one of the documented enum values and matches process.arch
  - os.platform() is one of the documented enum values and matches process.platform
  - os.type() returns a non-empty string ('Linux'/'Darwin'/'Windows_NT' depending on CI runner)
  - os.machine() returns a non-empty string, distinct enum from os.arch() (e.g. 'x86_64' vs 'x64' when applicable)
  - os.endianness() is 'LE' or 'BE'
  - os.release()/os.version() return non-empty strings
  - os.EOL === '\n' on POSIX runners, '\r\n' on Windows runners (platform-conditional assertion)
  - os.devNull === '/dev/null' on POSIX, '\\\\.\\nul' on Windows

tests/node/os/os_memory_load.test.ts
  - os.totalmem() > 0 and os.freemem() > 0 and os.freemem() <= os.totalmem()
  - os.uptime() > 0
  - os.loadavg() returns an array of exactly 3 numbers; all zero on Windows, at least one >= 0 on POSIX
  - os.availableParallelism() > 0 and os.availableParallelism() <= os.cpus().length
  - repeated calls to freemem()/uptime() are non-decreasing/plausible (uptime strictly increases across two calls with a sleep between)

tests/node/os/os_cpus.test.ts
  - os.cpus() returns a non-empty array (unless explicitly running in a stripped container — document skip condition)
  - each entry has model (non-empty string), speed (number >= 0), times.{user,nice,sys,idle,irq} all numbers >= 0
  - times.nice === 0 is NOT asserted on Windows removed as a field expectation (still present, but conventionally 0) — assert field exists and is a number, not that it's always 0, to avoid over-fitting Node's own doc caveat

tests/node/os/os_network_interfaces.test.ts
  - os.networkInterfaces() returns an object with at least a loopback entry (platform loopback name varies: 'lo'/'lo0'/'Loopback...')
  - the loopback entry has internal === true
  - non-loopback entries (if any) have internal === false
  - each entry has family 'IPv4' or 'IPv6' and address/netmask/mac all non-empty strings
  - a malformed/edge netmask case yields cidr === null rather than throwing (may require a mocked/fixture path if no such interface exists on the CI runner)

tests/node/os/os_user_info.test.ts
  - os.userInfo() returns { username, uid, gid, shell, homedir } with correct types
  - on POSIX: uid/gid are >= 0, shell is a non-empty string (path)
  - on Windows: uid === -1, gid === -1, shell === null
  - os.userInfo({ encoding: 'buffer' }) returns username/shell/homedir as Buffer instances (instanceof Uint8Array) that decode back to the same string as the default-encoding call
  - os.homedir() equals os.userInfo().homedir on POSIX (both derive from the same source)

tests/node/os/os_tmpdir.test.ts
  - os.tmpdir() returns a non-empty string with no trailing path separator
  - honors TMPDIR override on POSIX / TEMP override on Windows (set env var before spawning the test process, or via a subprocess harness)

tests/node/os/os_priority.test.ts
  - os.getPriority() (no pid) returns a number in [-20, 19] for the current process
  - os.setPriority(os.constants.priority.PRIORITY_BELOW_NORMAL) then os.getPriority() reflects a bucketed value >= PRIORITY_NORMAL's nice value (allow for OS-level rounding)
  - os.getPriority(999999) (a pid unlikely to exist) throws with err.code === 'ESRCH'
  - os.setPriority(os.constants.priority.PRIORITY_HIGHEST) without elevated privileges throws EACCES/EPERM on a restricted CI runner (document as a conditional/best-effort assertion, since CI privilege level varies)

tests/node/os/os_constants.test.ts
  - os.constants.signals.SIGINT/SIGTERM/SIGKILL are defined numbers on POSIX; SIGBREAK is defined on Windows and absent (key not present) on POSIX
  - os.constants.errno.ENOENT/EACCES/EEXIST are defined numbers on all platforms; WSAEACCES etc. are defined only on Windows (absent key on POSIX)
  - os.constants.dlopen has RTLD_LAZY/RTLD_NOW/RTLD_GLOBAL/RTLD_LOCAL on POSIX; is an empty object on Windows
  - os.constants.priority matches the documented 6-value table exactly (PRIORITY_LOW=19 .. PRIORITY_HIGHEST=-20)
  - os.constants.libuv.UV_UDP_REUSEADDR is a defined number
  - the constants object (or its sub-objects) is effectively frozen/consistent across repeated os.constants accesses within one process

tests/node/os/os_worker_threads.test.ts (multithread)
  - a main-thread os.setPriority(0, os.constants.priority.PRIORITY_LOW) is observable via os.getPriority(0) from a spawned worker_thread (process-wide side effect, NOT thread-local — demonstrates the 5.4 semantics)
  - concurrent os.cpus()/os.freemem()/os.networkInterfaces() calls from N worker threads simultaneously with the main thread all return internally-consistent (non-corrupted/non-interleaved) results with no crashes (stress test: 8 workers × 100 calls each)
  - os.hostname()/os.arch()/os.platform() return identical values across main thread and every worker thread (host-global facts, no per-thread override)
```

## 7. Open questions / deferrals

- **cgroup-aware `availableParallelism()` on Linux.** `std::thread::available_parallelism()` respects CPU-affinity masks but does not, by itself, parse `/sys/fs/cgroup/cpu.max` (v2) or `cpu.cfs_quota_us`/`cpu.cfs_period_us` (v1) the way Node/libuv's `uv_available_parallelism()` does. Needs an owner decision on whether to hand-roll this cgroup parsing in `rts-node` or pull in a small focused crate for it (e.g. something in the `num_cpus`/`cgroups-rs` space) — flagging rather than blocking since the plain-affinity-aware base already covers the non-containerized case correctly.
- **Exact per-platform `os.version()`/`os.release()` string format fidelity.** Node sources these from libuv's `uv_os_uname()`, which itself has platform-specific quirks (e.g. macOS `version` includes a build-metadata suffix that varies by Xcode/SDK version). RTS should aim for "a truthful, non-empty platform version string" rather than byte-for-byte matching Node's exact formatting, since Node itself doesn't guarantee a stable format across OS point-releases.
- **`os.cpus()` `model`/`speed` accuracy on ARM and virtualized/cloud hosts.** Some ARM SoCs and most cloud VM hypervisors report `speed: 0` or a placeholder model string upstream in the OS itself (not a Node or RTS bug) — the test plan's "speed >= 0" assertion is deliberately loose to avoid over-fitting real hardware that RTS cannot control.
- **Windows registry access for per-core CPU model name** (`HKEY_LOCAL_MACHINE\HARDWARE\DESCRIPTION\System\CentralProcessor\%d\~MHz` etc.) needs verification that RTS's sandboxed/restricted execution contexts (if any exist in the future) can still read this key; flagged as verify-on-implementation, not a blocking design question.
- **Whether `EOL`/`devNull` warrant a native call at all** versus being computed purely in the `.ts` shim from `os.platform() === 'win32'` — functionally equivalent either way; left as an implementation-detail choice in phase (b), not a parity-relevant decision.
- **cross-compiled `os.constants` correctness for the Windows `WSA*` errno table when building on a non-Windows host** (per the per-target runtime-archive cross-compile prep in `rts-linker`) — needs verification that the `libc`/`windows-sys` constant values are available at compile time from a cross-compilation host without requiring an actual Windows SDK install; flagged for whoever picks up the cross-compile leg of this module.
