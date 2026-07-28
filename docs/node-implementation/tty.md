# node:tty

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:tty` |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P1 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import tty from "node:tty"`; `import { isatty, ReadStream, WriteStream } from "node:tty"`; CJS `require("node:tty")` / legacy bare `require("tty")` |
| Globals exposed | None directly — `node:tty` adds nothing to `globalThis`. However, when Node/RTS detects it is attached to a real terminal, `process.stdin` is by default an instance of `tty.ReadStream` and `process.stdout`/`process.stderr` are by default instances of `tty.WriteStream`. That wiring is `node:process`'s responsibility (constructing the right class based on `tty.isatty(fd)`), not something `node:tty` itself installs as a global. |

## 1. Purpose

`node:tty` provides low-level access to the terminal (TTY) device attached to
a file descriptor: querying whether an fd is a TTY at all (`tty.isatty`),
reading/writing raw terminal state (`ReadStream.setRawMode`/`isRaw`), querying
and reacting to terminal geometry (`WriteStream.columns`/`rows`/
`getWindowSize()`/`'resize'`), and emitting cursor-control/screen-clearing
ANSI-equivalent operations (`clearLine`, `clearScreenDown`, `cursorTo`,
`moveCursor`) plus terminal color-capability detection
(`getColorDepth`/`hasColors`). It is the module that makes `process.stdin`
character-at-a-time-capable and lets CLI/TUI libraries (progress bars,
prompts, spinners, full-screen apps) control the terminal directly. Both
exported classes are thin specializations of `net.Socket` bound to a TTY file
descriptor — `node:tty` itself contributes only the TTY-specific
properties/methods layered on top of the generic socket/stream behavior.

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `class ReadStream extends net.Socket`

- **Added:** v0.5.8
- Not typically constructed directly by user code — `process.stdin` is the
  canonical instance when stdin is attached to a real terminal — but the
  constructor is public.

**Constructor:**

| Signature | Added |
|---|---|
| `new tty.ReadStream(fd: number, options?: net.SocketConstructorOpts)` | v0.5.8 (`options` since v0.9.4) |

| Param | Type | Optional | Default |
|---|---|---|---|
| `fd` | `number` | no | — a file descriptor associated with a TTY |
| `options` | `net.SocketConstructorOpts` | yes | — forwarded verbatim to the `net.Socket` constructor (fields such as `allowHalfOpen`, `readable`, `writable` — see the forward-referenced `node:net` spec) |

Returns: `tty.ReadStream`.

**Instance properties:**

| Property | Type | Added | Notes |
|---|---|---|---|
| `isRaw` | `boolean` | v0.7.7 | `true` iff the TTY is currently configured as a raw device. **Always `false` at process start**, even if the OS-level terminal happens to already be in raw mode from a previous process — it only changes via `setRawMode()`. |
| `isTTY` | `boolean` | v0.5.8 | **Always `true`** for any `tty.ReadStream` instance — this is a class-level constant, not a live check of the underlying `fd`. Constructing a `ReadStream` over a non-TTY fd does not make `isTTY` `false` (documented quirk, §4). |

**Instance methods:**

| Method | Signature | Added |
|---|---|---|
| `setRawMode` | `setRawMode(mode: boolean): this` | v0.7.7 |

**`setRawMode(mode)`**

| Param | Type | Optional | Default |
|---|---|---|---|
| `mode` | `boolean` | no | — `true` ⇒ raw device mode, `false` ⇒ default (cooked) mode |

Returns: `this` (the `ReadStream`, chainable). Throws: none documented for
the common case (implementations may raise on an invalid/closed fd — treat
as platform errno, §4). Variant: **sync**. Side effects when `true`://
input delivered byte-by-byte with no line buffering, all echoing/special
character processing disabled, and **Ctrl+C no longer raises `SIGINT`**
(the raw `0x03` byte is delivered to the stream instead). Does not affect
output-side processing (e.g. `\n`→`\r\n` translation is a POSIX terminal
output-mode concept, orthogonal to input raw mode).

**Events:** `ReadStream` defines no TTY-specific events of its own. All
events come from its `net.Socket`/`stream.Readable` ancestry (`'data'`,
`'end'`, `'close'`, `'error'`, `'readable'`, `'pause'`, `'resume'`, etc.) —
out of scope for this spec; see the forward-referenced `node:net`/stream
base spec (§5.6, §7).

---

#### `class WriteStream extends net.Socket`

- **Added:** v0.5.8
- Not typically constructed directly — `process.stdout`/`process.stderr`
  are the canonical instances when those fds are attached to a real
  terminal — but the constructor is public.

**Constructor:**

| Signature | Added |
|---|---|
| `new tty.WriteStream(fd: number)` | v0.5.8 |

| Param | Type | Optional | Default |
|---|---|---|---|
| `fd` | `number` | no | — a file descriptor associated with a TTY |

Returns: `tty.WriteStream`. (No `options` parameter, unlike `ReadStream`.)

**Instance properties:**

| Property | Type | Added | Notes |
|---|---|---|---|
| `columns` | `number` | v0.7.7 | Current terminal width in columns. Updated on every `'resize'` event. |
| `rows` | `number` | v0.7.7 | Current terminal height in rows. Updated on every `'resize'` event. |
| `isTTY` | `boolean` | v0.5.8 | **Always `true`** — same class-level-constant caveat as `ReadStream.isTTY`. |

**Instance methods (7 total):**

| Method | Signature |
|---|---|
| `clearLine` | `clearLine(dir: -1 \| 0 \| 1, callback?: () => void): boolean` |
| `clearScreenDown` | `clearScreenDown(callback?: () => void): boolean` |
| `cursorTo` | `cursorTo(x: number, y?: number, callback?: () => void): boolean` |
| `cursorTo` (overload) | `cursorTo(x: number, callback: () => void): boolean` |
| `moveCursor` | `moveCursor(dx: number, dy: number, callback?: () => void): boolean` |
| `getWindowSize` | `getWindowSize(): [number, number]` |
| `getColorDepth` | `getColorDepth(env?: Record<string, string \| undefined>): number` |
| `hasColors` | `hasColors(count?: number, env?: Record<string, string \| undefined>): boolean` |
| `hasColors` (overload) | `hasColors(env?: Record<string, string \| undefined>): boolean` |

**`clearLine(dir, callback?)`** — added v0.7.7 (callback/return value exposed
since v12.7.0).

| Param | Type | Optional | Default |
|---|---|---|---|
| `dir` | `-1 \| 0 \| 1` | no | — `-1`: clear left of cursor; `1`: clear right of cursor; `0`: clear entire line |
| `callback` | `() => void` | yes | — invoked once the write completes |

Returns: `boolean` — `false` if the caller should wait for `'drain'` before
writing more, `true` otherwise (same contract as `stream.Writable.write()`,
because internally this **is** a `write()` call carrying an ANSI/Console-API
payload). Variant: **callback**, stream-backpressure-aware.

**`clearScreenDown(callback?)`** — added v0.7.7 (callback/return exposed
v12.7.0). No positional params beyond the optional callback. Returns
`boolean` (same semantics as above). Clears the screen from the current
cursor position downward. Variant: **callback**.

**`cursorTo(x, y?, callback?)`** — added v0.7.7 (callback/return exposed
v12.7.0).

| Param | Type | Optional | Default |
|---|---|---|---|
| `x` | `number` | no | — target column |
| `y` | `number` | yes | — target row; omit to move only the column on the current row |
| `callback` | `() => void` | yes | — invoked once the write completes |

Returns: `boolean` (same semantics). Two call shapes: `cursorTo(x, y,
callback?)` and `cursorTo(x, callback)` (callback in the `y` position when
no row is given). Variant: **callback**.

**`moveCursor(dx, dy, callback?)`** — added v0.7.7 (callback/return exposed
v12.7.0).

| Param | Type | Optional | Default |
|---|---|---|---|
| `dx` | `number` | no | — horizontal distance, relative to current position |
| `dy` | `number` | no | — vertical distance, relative to current position |
| `callback` | `() => void` | yes | — invoked once the write completes |

Returns: `boolean` (same semantics). Moves the cursor **relative** to its
current position (unlike `cursorTo`, which is absolute). Variant:
**callback**.

**`getWindowSize()`** — added v0.7.7. No params.

Returns: `[number, number]` — `[numColumns, numRows]`, i.e. the same data
as `columns`/`rows` but as a fresh array snapshot rather than the cached
properties. Throws: none documented (an internal ioctl/console-API failure
would surface as a Node internal error — treat as platform errno, §4).
Variant: **sync**.

**`getColorDepth(env?)`** — added v9.9.0.

| Param | Type | Optional | Default |
|---|---|---|---|
| `env` | `Record<string, string \| undefined>` | yes | `process.env` — lets the caller simulate a different terminal's environment |

Returns: `number` — one of `1` (2 colors), `4` (16 colors), `8` (256
colors), `24` (16,777,216 colors / truecolor). Heuristic-based: can produce
false positives/negatives since it infers capability from environment
variables and process info, not a real terminal capability query.
Recognized overrides: `FORCE_COLOR=0` → 2 colors (`1`), `FORCE_COLOR=1` →
16 colors (`4`), `FORCE_COLOR=2` → 256 colors (`8`), `FORCE_COLOR=3` →
16.7M colors (`24`); `NO_COLOR` and `NODE_DISABLE_COLORS` both disable color
support regardless of other signals. Variant: **sync**.

**`hasColors(count?, env?)`** / **`hasColors(env?)`** — added v11.13.0 /
v10.16.0.

| Param | Type | Optional | Default |
|---|---|---|---|
| `count` | `integer` | yes | `16` — minimum color count being asked about (floor of 2) |
| `env` | `Record<string, string \| undefined>` | yes | `process.env` |

Returns: `boolean` — `true` iff the stream supports at least `count`
colors. Same false-positive/negative caveats as `getColorDepth()`. Variant:
**sync**.

**Events:**

| Event | Listener signature | Added | Fires when |
|---|---|---|---|
| `'resize'` | `() => void` | v0.7.7 | Whenever `columns` or `rows` changes. No arguments — listeners must re-read `writeStream.columns`/`.rows` (or call `getWindowSize()`) to learn the new size. |

All other events (`'close'`, `'error'`, `'drain'`, `'finish'`, `'pipe'`,
`'unpipe'`, …) come from the `net.Socket`/`stream.Writable` ancestry, out of
scope here (§5.6, §7).

### 2.2 Top-level functions

| Function | Signature |
|---|---|
| `isatty` | `isatty(fd: number): boolean` |

**`isatty(fd)`** — added v0.5.8.

| Param | Type | Optional | Default |
|---|---|---|---|
| `fd` | `number` | no | — a numeric file descriptor |

Returns: `boolean` — `true` iff `fd` is associated with a TTY, `false`
otherwise (including when `fd` is not a non-negative integer — no throw
for a malformed input, just `false`). Throws: none. Variant: **sync**.

This is the module's **only** top-level function (`functionCount = 1`).

### 2.3 Properties & constants

None. `node:tty` exports no module-level constants or properties (unlike
`fs.constants`/`os.constants`/`crypto.constants`). Historical module-level
`tty.setRawMode()`/`tty.getWindowSize()` free functions existed as thin
proxies in very early (pre-v1) Node but are **not** part of the current
(v25) surface — do not implement them; only the class-instance methods
above exist today. *(verify exact removal version if ever needed for a
compat shim — not required for this spec.)*

### 2.4 Events

Summarized from §2.1 — `node:tty` itself introduces exactly **one** event:

| Event | Owner class | Listener signature |
|---|---|---|
| `'resize'` | `WriteStream` | `() => void` |

`ReadStream` introduces no TTY-specific event. Both classes' full event
surfaces are dominated by their `net.Socket` ancestry, which is out of
scope for this document (tracked as a cross-module dependency in §5.6/§7).

## 3. Types & option objects

```typescript
// Re-exported/forward-referenced shape from the future node:net spec —
// tty.ReadStream's constructor forwards `options` verbatim to net.Socket.
interface SocketConstructorOpts {
  fd?: number;
  allowHalfOpen?: boolean;
  readable?: boolean;
  writable?: boolean;
  signal?: AbortSignal;
}

type ClearLineDir = -1 | 0 | 1;

type StreamOpCallback = () => void;

// getColorDepth/hasColors env snapshot — any subset of process.env is valid,
// values may be undefined (an unset variable), never null.
type EnvSnapshot = Record<string, string | undefined>;

// getWindowSize()'s return shape (a tuple, not an object)
type WindowSize = [columns: number, rows: number];

// Internal-only (not part of the public API surface, used by this spec's
// RTS notes below to describe how getColorDepth()'s return value is
// classified):
type ColorDepth = 1 | 4 | 8 | 24;
```

Note: unlike `dgram.AddressInfo`/`MessageInfo`, `node:tty` has no returned
"info object" shapes at all — every return value is either a primitive
(`boolean`, `number`), a plain tuple (`WindowSize`), or `this` (chaining).

## 4. Node semantics & edge cases

### `isTTY` is a class constant, not a live probe

Both `ReadStream.isTTY` and `WriteStream.isTTY` are **hardcoded `true`** on
the class — they do not re-check `isatty(fd)` at read time. If user code
manually constructs `new tty.WriteStream(someNonTtyFd)`, `.isTTY` still
reads `true`even though the underlying fd is not really a terminal. The
*correct* way to check "is this fd really a TTY" is the module-level
`tty.isatty(fd)` function, not the instance's `.isTTY` property (which only
tells you "this is an instance of the TTY-flavored stream class").

### `isRaw` starts `false` even in an already-raw terminal

`readStream.isRaw` reflects **RTS/Node's own tracked state**, initialized to
`false` on every process start, regardless of what raw/cooked mode the real
terminal device was already in before the process launched. It only changes
in response to `setRawMode()` calls made by this process.

### Raw mode specifics

- Raw mode affects **input** processing only: character-by-character
  delivery, no line editing, no echo. It does **not** affect terminal
  **output** processing (e.g. `\n` → `\r\n` newline translation on POSIX
  terminals is independent and unaffected).
- **Ctrl+C stops delivering `SIGINT`** while in raw mode — the raw byte
  `0x03` is delivered to the input stream instead. Applications that enable
  raw mode for interactive UIs are responsible for detecting `0x03`
  themselves if they want Ctrl+C-like behavior.
- Calling `setRawMode()` on a non-TTY fd, or on a platform/terminal that
  does not support the requested mode, is a source of platform-dependent
  errno-class errors (§ error table below) — not a hard Node.js-specific
  error code.

### Windows vs POSIX implementation differences

Node's own TTY backend (via libuv) differs materially by platform; RTS's
native implementation must reproduce the **observable behavior**, not any
particular internal mechanism:

| Concern | POSIX | Windows |
|---|---|---|
| Raw mode toggle | `tcgetattr`/`tcsetattr` with `termios` flags (`ECHO`, `ICANON`, `ISIG`, etc. cleared; mirrors `cfmakeraw`) | `GetConsoleMode`/`SetConsoleMode` on the console input handle, clearing `ENABLE_ECHO_INPUT`/`ENABLE_LINE_INPUT`/`ENABLE_PROCESSED_INPUT` |
| Window size query | `ioctl(fd, TIOCGWINSZ, &winsize)` | `GetConsoleScreenBufferInfo` on the console output handle |
| Resize notification | `SIGWINCH` signal | No native resize signal — requires either polling `GetConsoleScreenBufferInfo` on an interval, or a dedicated thread reading `WINDOW_BUFFER_SIZE_EVENT` records via `ReadConsoleInputW` on the input handle (libuv on Windows uses the latter) |
| ANSI escape codes (cursor/clear) | Always supported by the real terminal (or the terminal's own emulator) | Supported natively since Windows 10 (build 1511+) if `ENABLE_VIRTUAL_TERMINAL_PROCESSING` is successfully enabled via `SetConsoleMode`; older `cmd.exe`/`conhost.exe` needs the legacy Console API (`SetConsoleCursorPosition`, `FillConsoleOutputCharacterW`, `ScrollConsoleScreenBufferW`) as a fallback |
| `isatty` | `isatty(3)` libc call | `GetFileType`/`GetConsoleMode` check on the underlying `HANDLE` |

### Color-capability detection is a best-effort heuristic

`getColorDepth()`/`hasColors()` are explicitly documented as capable of
**false positives and false negatives** — they infer support from
environment variables (`FORCE_COLOR`, `NO_COLOR`, `NODE_DISABLE_COLORS`,
and implicitly others like `TERM`/`COLORTERM`/CI-detection variables per
Node's internal heuristic) plus the `isatty()` status of the underlying fd,
not a real terminfo/capability query. `env` overrides let a caller *simulate*
another terminal's environment (useful for CI or piping-to-a-file
scenarios) without touching `process.env` itself.

### Error / errno reference

| Code | Raised by | Meaning |
|---|---|---|
| `EBADF` | any method, if `fd` is closed/invalid | Bad file descriptor. |
| `ENOTTY` | conceptually, when an ioctl/console call is attempted on a non-TTY fd | In practice Node/RTS should prefer returning `false`/a safe default (as `isatty()` already does) over throwing, mirroring the "no throw" contract of `isatty()`; `ENOTTY` is documented here for completeness of the underlying syscall surface, not as a Node-facing error. |
| `EINVAL` | `clearLine(dir)` | If `dir` is outside `{-1, 0, 1}` (Node's TS types constrain this at compile time; RTS's native layer must still validate at runtime for `any`-typed/untrusted call sites). |

### Ordering / backpressure

`clearLine`/`clearScreenDown`/`cursorTo`/`moveCursor` all follow the
`stream.Writable.write()` contract: a `false` return means the caller
*should* wait for `'drain'` before issuing more writes, though nothing
prevents calling again immediately (data is buffered, not dropped). Real
Node routes these through the same internal write queue as any other
`stream.write()` call on the underlying `net.Socket` — see §5.3 for how RTS
approximates this before a full `node:net`/stream base exists.

### Deprecations

No methods in the current (v25) `node:tty` surface are deprecated. The
`rinfo`-style version-history churn seen in `node:dgram` has no analogue
here — `tty`'s documented surface has been stable in shape for a long time
(only new callback/return-value **exposure** in v12.7.0, not a shape
change).

### Security notes

Not a traditional attack-surface module, but worth flagging operationally:
enabling raw mode disables the terminal's own `Ctrl+C` → `SIGINT`
short-circuit, so a process that enables raw mode and then hangs (bug,
infinite loop, deadlocked await) becomes materially harder for an
interactive user to kill via the keyboard alone (they must use a different
signal/mechanism, e.g. `Ctrl+\` → `SIGQUIT` on POSIX where still delivered,
or an external `kill`/Task Manager). RTS's `.ts` shim should always restore
cooked mode on process exit (normal and signal-triggered) as a safety net —
see §5.8.

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node` is fully independent — no `rts-std` dependency. `node:tty`'s
native surface is unusually small: every operation is either a cheap
syscall/ioctl keyed by an already-open fd, or a handful of bytes written
directly to that fd. No new heap-allocated OS resource is created by this
module (unlike `dgram`'s sockets) — the fd itself, owned by the process
(typically 0/1/2), **is** the identity.

| Surface area | Backing |
|---|---|
| `isatty(fd)` | **`std::io::IsTerminal`** (stable since Rust 1.70, part of `std` — zero new dependency). Works on any `AsFd` (Unix) / `AsHandle` (Windows) — wrap the raw `i32`/`RawFd` via `std::os::fd::BorrowedFd::borrow_raw` (Unix) or the Windows equivalent `BorrowedHandle` before calling `.is_terminal()`. Preferred over hand-rolling `libc::isatty`/`GetConsoleMode` directly, and over pulling in the `is-terminal`/`crossterm` crates. |
| Raw mode get/set | POSIX: raw `libc::tcgetattr`/`libc::tcsetattr` (`libc` crate — already an accepted `rts-node` dependency per the `dgram` spec) manipulating the `termios` struct's `c_lflag`/`c_iflag`/`c_oflag`/`c_cflag` (mirror `cfmakeraw` semantics: clear `ECHO`,`ICANON`,`ISIG`,`IEXTEN`,`BRKINT`,`ICRNL`,`INPCK`,`ISTRIP`,`IXON`, set `CS8`). Windows: `windows-sys` crate's `Win32::System::Console::{GetConsoleMode, SetConsoleMode}` on the console **input** handle, toggling `ENABLE_ECHO_INPUT`/`ENABLE_LINE_INPUT`/`ENABLE_PROCESSED_INPUT`. |
| Window size (`columns`/`rows`/`getWindowSize()`) | POSIX: `libc::ioctl(fd, TIOCGWINSZ, &mut winsize)`. Windows: `windows-sys`'s `GetConsoleScreenBufferInfo` on the console **output** handle, computing `columns = srWindow.Right - srWindow.Left + 1`, `rows = srWindow.Bottom - srWindow.Top + 1`. |
| Resize notification (`'resize'`) | POSIX: install a `SIGWINCH` handler (`libc::signal`/`sigaction`, or the `signal-hook` crate for a safer registration pattern) that flags a per-fd atomic dirty bit; the `.ts` shim polls it once per event-loop tick. Windows: no equivalent signal — either (a) poll `GetConsoleScreenBufferInfo` on an interval from a dedicated background thread, or (b) a background thread blocking on `ReadConsoleInputW` for the input handle and filtering for `WINDOW_BUFFER_SIZE_EVENT` records (mirrors libuv's own Windows approach, more efficient than polling — **preferred**, mark exact ergonomics `(verify)` during implementation). |
| Cursor/clear ops (`clearLine`, `clearScreenDown`, `cursorTo`, `moveCursor`) | POSIX + Windows-with-VT-enabled: hand-build the ANSI escape sequence natively (`\x1b[{n}G` absolute column, `\x1b[{n};{m}H` absolute position, `\x1b[{n}A/B/C/D` relative movement, `\x1b[K`/`\x1b[1K`/`\x1b[2K` line-clear variants, `\x1b[J` screen-clear-down) and write it directly to the fd with a raw `write(2)`/`WriteFile` call — no crate needed, this mirrors Node's own `lib/internal/readline/utils.js` string-building approach. Windows-without-VT (legacy `cmd.exe`): fallback to `windows-sys`'s `SetConsoleCursorPosition`/`FillConsoleOutputCharacterW`/`ScrollConsoleScreenBufferW`. |
| VT100 negotiation on Windows | Attempt `SetConsoleMode(..., ENABLE_VIRTUAL_TERMINAL_PROCESSING)` once per output handle at first use; cache success/failure; fall back to the legacy Console API path on failure (older Windows 10 builds / non-conhost hosts). |
| `getColorDepth`/`hasColors` | **Pure `.ts` logic** — no native extern needed beyond `isatty()` (see §5.2). The env-var heuristic (`FORCE_COLOR`/`NO_COLOR`/`NODE_DISABLE_COLORS`/`TERM`/`COLORTERM`/CI-detection) is string/env manipulation only; there is no OS syscall involved. This is a deliberate simplification opportunity flagged explicitly rather than silently — matches the "don't implement high-level APIs in Rust" design rule directly. |

### 5.2 ABI surface

`ns_prefix = "node_tty"`, `node_module = "tty"`, registered in `rts-node`'s
`NODE_SPECS` exactly like `fs`/`path`/`os`/`process`/`util`/`crypto` today
(`crates/rts-node/src/lib.rs`). Unlike `dgram`, **no `Handle` is needed** —
every operation is keyed by a plain `fd: I32` (or `I64` if a wider type is
preferred for future-proofing; `I32` matches the OS's own fd/HANDLE-index
width and Node's own `number` fd type closely enough). Per-fd native-only
bookkeeping that has no JS-visible identity of its own (whether a
`SIGWINCH`/resize-watcher thread is running for that fd, cached "VT100
supported" flag for a Windows output handle) lives in an internal
`OnceLock<Mutex<HashMap<i32, TtyFdState>>>` inside `rts-node`'s own `tty`
module — never exposed as a `Handle` across the ABI, since it is pure
implementation-detail state, not a user-visible resource with its own
lifecycle the JS side needs to free.

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_TTY_ISATTY` | `I32(fd)` | `Bool` | Backs `tty.isatty()`. Never throws — invalid `fd` simply returns `false`. |
| `__RTS_FN_NODE_TTY_GET_WINDOW_SIZE` | `I32(fd)` | `U64` (packed: high 32 bits = columns, low 32 bits = rows) | One packed return avoids a second extern/out-param for a 2-tuple; the `.ts` shim unpacks into `[columns, rows]` / the `columns`/`rows` properties. |
| `__RTS_FN_NODE_TTY_SET_RAW_MODE` | `I32(fd), Bool(mode)` | `I32` (0 = ok, else negated errno) | The `.ts` shim tracks `isRaw` itself from the call's own argument on success (mirrors Node: `isRaw` is RTS/Node-tracked state, not re-queried from the OS every time, per §4). |
| `__RTS_FN_NODE_TTY_CLEAR_LINE` | `I32(fd), I32(dir)` | `I32` (status) | `dir ∈ {-1, 0, 1}`; out-of-range validated native-side (`EINVAL`) even though TS types constrain it, since `any`-typed/untrusted call sites must not bypass the check (design doc's coercion-boundary rule). |
| `__RTS_FN_NODE_TTY_CLEAR_SCREEN_DOWN` | `I32(fd)` | `I32` (status) | |
| `__RTS_FN_NODE_TTY_CURSOR_TO` | `I32(fd), I32(x), I32(y), Bool(has_y)` | `I32` (status) | `has_y = false` ⇒ move only the column on the current row (mirrors the `cursorTo(x, callback)` overload). |
| `__RTS_FN_NODE_TTY_MOVE_CURSOR` | `I32(fd), I32(dx), I32(dy)` | `I32` (status) | |
| `__RTS_FN_NODE_TTY_RESIZE_WATCH_START` | `I32(fd)` | `I32` (status) | Idempotent: starts the background `SIGWINCH` handler registration (POSIX) or the `ReadConsoleInputW` watcher thread (Windows) for this fd if not already running; ref-counted internally by listener count so multiple `WriteStream`s on the same fd share one watcher. |
| `__RTS_FN_NODE_TTY_RESIZE_WATCH_STOP` | `I32(fd)` | `Void` | Decrements the ref count; tears down the watcher when it hits zero. |
| `__RTS_FN_NODE_TTY_RESIZE_POLL` | `I32(fd)` | `Bool` (a resize was observed since the last poll) | Drains the per-fd dirty bit; called once per event-loop tick per `WriteStream` with a `'resize'` listener (same poll-per-tick pattern as `dgram`'s `POLL`/`CURRENT_MSG_*` externs). |

No `getColorDepth`/`hasColors` externs exist — per §5.1 these are
implemented **entirely** in the `.ts` shim over `ISATTY` + a plain JS object
read of `env`/`process.env`, with zero native surface. This is the
opposite choice from `dgram`, where every operation needed a native
syscall; `tty`'s color-heuristic genuinely needs none.

### 5.3 Async model

- **Everything in this module is synchronous at the native layer** —
  `isatty`, raw-mode toggling, window-size queries, and the cursor/clear
  writes are all fast, non-blocking OS calls (an `ioctl`/`GetConsoleMode`-
  class syscall or a tiny direct `write(2)`/`WriteFile` of a handful of
  ANSI bytes). None of them need `spawn_blocking` on a shared tokio
  runtime, and there is **no Promise-based API surface anywhere in
  `node:tty`**.
- **`clearLine`/`clearScreenDown`/`cursorTo`/`moveCursor`'s callback +
  `boolean` return** is documented as riding on the real `net.Socket`
  write-queue/backpressure machinery (§4). Until a real `node:net`/
  `stream.Duplex` base exists (§5.6, §7), RTS's v1 implementation performs
  the native write **directly and synchronously** on the calling thread,
  always returns `true` (no backpressure modeling — an honest, explicitly
  flagged simplification, not a silent one), and invokes the optional
  `callback` immediately (or via one microtask tick, TBD at implementation
  time, to avoid same-tick callback surprises some code may depend on).
  Revisit once the real stream base lands (§7).
- **The `'resize'` event** needs a genuinely async source (a signal handler
  or a background watcher thread, §5.1) even though the module's own ABI
  calls are synchronous polls. The `.ts` shim calls
  `__RTS_FN_NODE_TTY_RESIZE_POLL` once per event-loop tick for every
  `WriteStream` with at least one `'resize'` listener (mirrors `dgram`'s
  inbound-message poll pattern) and fires `'resize'` (with fresh
  `columns`/`rows` read via `GET_WINDOW_SIZE`) when it returns `true`.
- **Not needed**: the shared tokio runtime, the `PromiseSlot`/
  `promise.create` subsystem — nothing in `node:tty`'s documented surface
  returns a `Promise` or needs non-blocking DNS-style resolution.

### 5.4 Multithread / worker interaction

Per `docs/specs/rts-threading-model.md` (worker = RTS thread/region,
`MessagePort` = channel, `SharedArrayBuffer` = shared heap):

- `process.stdin`/`stdout`/`stderr` are **process-global, main-thread**
  concepts. Matching Node's own `worker_threads` behavior: a spawned RTS
  worker thread/region does **not** get its own raw TTY access — `console`
  output from a worker is piped back to the main thread/region (over a
  channel, conceptually a `MessagePort`) rather than writing directly to
  the real stdio fds, and `process.stdin` is simply not meaningfully
  available inside a worker. Only the main thread/region should construct
  or hold a live `tty.ReadStream`/`WriteStream` over the real stdio fds.
- The underlying OS resource (an fd, a termios/console-mode setting) is
  **process-wide state**, not per-thread — raw mode is a property of the
  terminal device itself, not of any one RTS thread. The internal
  `HashMap<i32, TtyFdState>` bookkeeping (§5.2) must therefore be a single
  shared, `Mutex`-guarded structure visible to every OS thread — small,
  low-contention metadata, not a violation of the per-thread-region memory
  model (it holds no user-visible heap values, only native flags/thread
  handles).
- **Hazard, not new**: two threads/regions racing to call `setRawMode()` on
  the *same* real fd is inherently a shared-mutable-OS-resource conflict
  that exists in POSIX/Windows terminals regardless of RTS — the threading
  model cannot make that safe, only avoid making it *worse*. The
  recommended posture (only the main thread/region touches real stdio
  fds) sidesteps the hazard entirely rather than trying to arbitrate it.
- A background resize-watcher thread (§5.1/§5.3) is an RTS **internal**
  implementation-detail thread, invisible to the JS-level threading model;
  it must be torn down via `RESIZE_WATCH_STOP` when the last listener is
  removed (or at process exit) to avoid a leaked OS thread/signal handler.

### 5.5 Buffer / TypedArray interop

**None.** `node:tty` moves no bulk binary payloads across the ABI at all —
every argument/return value is a small integer (`fd`, `dir`, `x`, `y`, `dx`,
`dy`, packed columns/rows), a `Bool`, or `this`-for-chaining. The
ANSI-escape byte sequences that `clearLine`/`cursorTo`/etc. ultimately write
to the fd are constructed **natively**, from the numeric arguments — no
string or byte buffer crosses the ABI boundary for those calls at all (not
even a `StrPtr`). The one place a Buffer-shaped concept is conceptually
adjacent — raw keystrokes arriving on `process.stdin` while in raw mode —
is entirely the job of the (not-yet-built) `net.Socket`/`stream.Readable`
read path that `ReadStream` inherits, not something `node:tty` itself
handles; out of scope for this document (§5.6, §7).

### 5.6 Doctrine placement

`node:tty` is unambiguously **non-primordial** — it has no native literal
syntax; a `ReadStream`/`WriteStream` is reached only via
`new tty.ReadStream(fd)`/`new tty.WriteStream(fd)` (ordinary constructor
calls) or via `node:process`'s internal wiring of `process.stdin`/`stdout`/
`stderr`. The engine must never hardcode the strings `"tty"`,
`"ReadStream"`, or `"WriteStream"` anywhere in `crates/rts-codegen-new/`.
Resolution path: `import ... from "node:tty"` → `ns_prefix_for("node:tty")`
→ `"node_tty"` → `node_lookup("node_tty.<member>")` → the matching
`NodespaceMember` in `rts-node`'s `tty::SPEC` — the exact same data-table
mechanism already wired for `fs`/`path`/`os`/`process`/`util`/`crypto`
(`crates/rts-node/src/lib.rs`'s `NODE_SPECS`/`node_lookup`/`ns_prefix_for`).
Adding `tty` support means adding one new `NodespaceSpec` entry, never
touching engine control flow.

The native/`.ts` split: the primitive ops in §5.2 (`ISATTY`,
`GET_WINDOW_SIZE`, `SET_RAW_MODE`, `CLEAR_LINE`, `CLEAR_SCREEN_DOWN`,
`CURSOR_TO`, `MOVE_CURSOR`, the `RESIZE_WATCH_*`/`RESIZE_POLL` trio) are
native `extern "C"` symbols; everything else — the `ReadStream`/
`WriteStream` classes themselves, `isRaw`/`columns`/`rows` property
caching, the `'resize'` event wiring over the poll extern, the entire
`getColorDepth`/`hasColors` heuristic (§5.1), and — critically — the
`net.Socket` base behavior these two classes are documented to extend —
all live in a `.ts` shim shipped by `rts-node`.

**Open cross-module dependency**: because `ReadStream`/`WriteStream`
*extend* `net.Socket` (itself `stream.Duplex`), and neither `node:net` nor
a stream base class exists yet in this codebase, this spec's class
definitions necessarily forward-reference an unbuilt base. `node:tty`'s own
native surface (§5.2) is fully independent of that gap and can be built and
tested standalone (treating `ReadStream`/`WriteStream` as plain classes
with no inherited stream behavior for v1), but full Node-compatible
behavior (piping, `.pipe()`, real backpressure/`'drain'`, `.write()`/`.end()`
on the write side, `'data'`/`'readable'` on the read side) is blocked on
that future work landing — tracked as an explicit deferral, not silently
ignored (§7).

### 5.7 Shared-infra dependencies (FLAG)

- **Persistent event-loop "keep polling while a listener is attached"
  semantics** — same gap already flagged for `node:dgram`. RTS's current
  event loop (`rts-std::event_loop::run_event_loop`) is a single bounded
  drain, not a persistent reactor. A `WriteStream` with a `'resize'`
  listener needs the process to keep polling
  `__RTS_FN_NODE_TTY_RESIZE_POLL` every tick indefinitely (until the
  listener is removed or the stream closes), which the current drain model
  does not provide. Cross-cutting infra `rts-node` cannot build
  unilaterally — needs resolution at the shared/engine level.
- **Ambient console/stdio fd ownership.** Today, the ambient `console`/
  `process.stdout` prelude writes to fd 1/2 through `rts-std`/
  `rts-runtime`'s existing `io` namespace (`__RTS_FN_NS_IO_PRINT`/
  `STDOUT_WRITE`/etc. — see `.claude/rules/01-architecture.md`'s `io/`
  section). If `node:tty`'s `WriteStream` (or `node:process`'s future
  `process.stdout` wiring) *also* writes to the same fd via its own native
  path, there are now two independent writers of the same OS file
  descriptor with no shared buffering/ordering coordination — a latent
  interleaving/ordering hazard, not a hypothetical one. This needs an
  explicit "single writer of fd 1/2" decision once `node:process`'s
  stdio wiring is speced, not something `node:tty` can resolve alone.
  Not a `rts-std`-dependency in the "must hoist a Rust module" sense, but
  flagged here as the closest analogous shared-infra coordination point.
- **Not needed**: the shared multi-thread tokio runtime, the Promise/
  `PromiseSlot` subsystem, and TLS/crypto primitives — `node:tty` has no
  async I/O, no DNS, no Promise-returning method, and no encryption
  anywhere in its surface (§5.3).

### 5.8 Implementation phases

a. **`isatty(fd)`** — the single top-level function, via
   `std::io::IsTerminal`; enough for `node:process` to decide which class
   to instantiate for `process.stdin`/`stdout`/`stderr` even before those
   classes are fully built.
b. **`WriteStream` skeleton + geometry** — `columns`/`rows`/
   `getWindowSize()` via the platform ioctl/Console-API calls (§5.1); no
   cursor ops, no resize events, no raw mode yet. Enough for a
   "print the terminal size" fixture.
c. **Cursor/clear operations** — `clearLine`/`clearScreenDown`/`cursorTo`/
   `moveCursor`, synchronous v1 semantics (§5.3): direct native write,
   always return `true`, callback invoked immediately/next-microtask.
   Includes the Windows VT100-negotiation-with-fallback logic (§5.1).
d. **`ReadStream.setRawMode`/`isRaw`** — POSIX `termios` + Windows console
   mode toggling; must restore cooked mode on normal **and**
   signal-triggered process exit as a safety net (§4 security note).
e. **`getColorDepth`/`hasColors`** — pure `.ts` heuristic layered over
   `ISATTY` + an env-var read; zero new native surface (§5.1/§5.2).
f. **`'resize'` event** — `RESIZE_WATCH_START`/`_STOP`/`RESIZE_POLL`
   externs, the POSIX `SIGWINCH`-handler-plus-dirty-bit implementation,
   and the Windows background-thread/`ReadConsoleInputW` implementation;
   wired into the event loop's per-tick poll (best-effort against the
   §5.7 event-loop gap, same caveat as `dgram`).
g. **Cross-module retrofit** — once a real `node:net`/`stream.Duplex` base
   lands, rebase `ReadStream`/`WriteStream` onto it for genuine
   backpressure/`'drain'`/`.pipe()` compliance, replacing phase (c)'s
   synchronous-write simplification.

## 6. Test plan

1. **`isatty()` on real stdio fds**: run once under a real terminal (manual/
   local verification, not CI-guaranteed) asserting `isatty(0/1/2)` is
   `true`; run under CI (piped/redirected stdio) asserting `false` — both
   branches must be exercised, not just the happy path.
2. **`isatty()` edge inputs**: negative fd, non-integer-like fd, and a
   closed/never-opened fd all return `false` without throwing.
3. **`WriteStream.getWindowSize()` / `.columns` / `.rows` consistency**:
   `getWindowSize()`'s tuple matches the cached `columns`/`rows`
   properties at the same instant.
4. **Cursor/clear byte-sequence correctness**: redirect the target fd to a
   pipe (not a real terminal) and assert the **exact** ANSI escape bytes
   written for each of `clearLine(-1|0|1)`, `clearScreenDown()`,
   `cursorTo(x)`, `cursorTo(x, y)`, `moveCursor(dx, dy)` — including
   negative `dx`/`dy` (movement should still resolve to the correct
   direction-flavored escape, e.g. `\x1b[{n}A` vs `\x1b[{n}B`).
5. **`clearLine`/`cursorTo`/etc. callback firing + return value**: assert
   the documented `boolean` return and that the optional callback fires
   exactly once.
6. **`setRawMode` round trip**: `setRawMode(true)` → `isRaw === true` →
   `setRawMode(false)` → `isRaw === false`; skip/mark best-effort in a
   non-TTY CI sandbox where the underlying syscall may no-op or error.
7. **Raw-mode Ctrl+C suppression** (best-effort/skip-if-unavailable in a
   pty-less CI harness): simulate delivering byte `0x03` while in raw mode
   and assert the process does **not** receive `SIGINT`, with the raw byte
   instead visible on the input stream.
8. **`isTTY` class-constant quirk**: construct a `WriteStream`/`ReadStream`
   over a deliberately non-TTY fd (e.g. a regular file or a pipe) and
   assert `.isTTY === true` regardless (matching the documented Node
   quirk, §4) while `tty.isatty(thatFd) === false`.
9. **`getColorDepth`/`hasColors` heuristic matrix**: `FORCE_COLOR` = `0`/
   `1`/`2`/`3`; `NO_COLOR` set (any value); `NODE_DISABLE_COLORS` set;
   an explicit simulated `env` object passed directly (e.g.
   `hasColors({ TMUX: '1' })` truthy per the Node example in the doc,
   `hasColors(2 ** 24, { TMUX: '1' })` falsy) — deterministic, no real
   terminal required.
10. **`'resize'` event fires and updates geometry**: force a synthetic
    resize (test harness invokes the native resize-watcher's underlying
    mechanism directly, or on POSIX raises `SIGWINCH` itself) and assert
    the listener fires with no arguments and `columns`/`rows` reflect the
    new size afterward.
11. **Multiple `'resize'` listeners on the same `WriteStream`/fd**: adding
    and removing listeners correctly starts/stops exactly one underlying
    watcher (ref-counted per §5.2), never double-starts or leaks.
12. **Multithread isolation**: spawn N RTS worker threads/regions; assert
    none of them construct a live `ReadStream`/`WriteStream` over the real
    stdio fds (per §5.4) and that `console`-style output from a worker is
    observed only via the piped/channel path on the main thread, never by
    directly racing the main thread's own stdio writes.
13. **Process-exit raw-mode restoration**: enable raw mode, then trigger
    both a normal exit and a signal-triggered exit (e.g. `SIGTERM` on
    POSIX) and assert the terminal is left in cooked mode afterward (the
    §4 security-note safety net) — best-effort/manual on platforms where
    CI cannot easily assert real terminal state.

## 7. Open questions / deferrals

- **`node:net`/stream base dependency** (§5.6) — `ReadStream`/`WriteStream`
  are documented to extend `net.Socket`, which does not exist in this
  codebase yet. Until it (or at least a minimal internal
  `stream.Duplex`-shaped base) lands, this spec's classes are implemented
  as standalone wrappers with the synchronous-write simplification (§5.3
  phase c), not fully spec-compliant streams (no real `.pipe()`, no real
  `'drain'`-driven backpressure, no `'data'` event on the read side). Owner
  decision needed on whether to retrofit later (recommended, tracked as
  phase (g)) or build a minimal shared stream base now, before `tty`, so
  both `node:tty` and the eventual `node:net` consume it from day one.
- **Ambient stdio fd ownership** (§5.7) — today's ambient console prelude
  already writes fd 1/2 via `rts-std`/`rts-runtime`'s `io` namespace. This
  spec's `WriteStream` writes the *same* fds via its own native path.
  Needs an explicit single-writer decision (likely: `node:process`'s
  future `process.stdout`/`stderr` wiring becomes the **only** entry point
  that ends up calling either path, with the ambient `console` global
  itself rewired on top of `node:tty`'s `WriteStream` rather than the
  legacy `io` namespace directly) before both co-exist in a shipped build.
- **Windows resize-detection mechanism** (§5.1) — polling
  `GetConsoleScreenBufferInfo` on an interval vs. a dedicated
  `ReadConsoleInputW`-blocking thread filtering `WINDOW_BUFFER_SIZE_EVENT`
  is proposed as the latter (matches libuv, more efficient) but is
  unverified end-to-end in this codebase — `(verify)` at implementation
  time, including behavior when the console has no window (e.g. a detached/
  redirected process).
- **Windows legacy (`cmd.exe`, no VT100) cursor/clear fallback path** — the
  `SetConsoleCursorPosition`/`FillConsoleOutputCharacterW`/
  `ScrollConsoleScreenBufferW` fallback is proposed but not verified for
  exact positional/column-vs-character-cell equivalence with the ANSI
  path — `(verify)`.
- **Exact `getColorDepth`/`hasColors` heuristic** — the public docs
  describe the *outcome* (`FORCE_COLOR`/`NO_COLOR`/`NODE_DISABLE_COLORS`
  overrides, `isatty`-gated) but Node's real internal heuristic
  (`lib/internal/tty.js`) additionally consults `TERM`, `COLORTERM`,
  CI-detection env vars (`CI`, `GITHUB_ACTIONS`, `TEAMCITY_VERSION`,
  `TRAVIS`, …), and terminal-program identification (`TERM_PROGRAM`,
  `TMUX`, Windows build number for the native truecolor cutover). The
  implementer should consult Node's own source for the precise precedence
  order rather than inventing one — `(verify)` against the pinned Node
  version's `lib/internal/tty.js` at implementation time.
- **Synchronous-write backpressure simplification** (§5.3 phase c) is an
  explicitly flagged, temporary divergence from the documented
  `stream.Writable`-backed contract — acceptable for v1, must be revisited
  once the `net.Socket`/stream base work lands (phase g), not left as a
  silent permanent gap.
- **`SIGWINCH` handler installation safety** — installing a signal handler
  from a library (as opposed to the application) always risks clobbering a
  handler the embedding application itself wants to install; RTS should
  document (and ideally provide an opt-out for) `node:tty`'s `SIGWINCH`
  handler registration once implemented, mirroring the general caution
  Node itself exercises around signal handling.
