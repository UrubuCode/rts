# node:readline

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:readline` (+ `node:readline/promises`) |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P1 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import readline from 'node:readline'`; `import * as readline from 'node:readline'`; `import { createInterface, Interface, emitKeypressEvents, clearLine, clearScreenDown, cursorTo, moveCursor } from 'node:readline'`; `const readline = require('node:readline')`; `import readlinePromises from 'node:readline/promises'`; `import { createInterface as createInterfacePromises, Interface as InterfacePromises, Readline } from 'node:readline/promises'`; `const readlinePromises = require('node:readline/promises')` |
| Globals exposed | none — all access is via the `node:readline` / `node:readline/promises` module imports; no ambient globals |

## 1. Purpose

`node:readline` reads a `Readable` stream (typically `process.stdin`, a file stream, or a socket) one line at a time and exposes it as an `Interface` — the foundation of interactive CLI tools and REPLs (`rl.question()`, `rl.prompt()`, command history, Tab completion, line editing keybindings). It also exposes low-level TTY cursor-control utilities (`clearLine`, `clearScreenDown`, `cursorTo`, `moveCursor`) usable against any writable TTY stream independent of an `Interface`, and `readline.emitKeypressEvents()`, which turns a raw input stream into a source of structured `'keypress'` events (arrow keys, Ctrl/Meta/Shift combinations) — the primitive that `Interface`'s own line editing is built on top of. `node:readline/promises` mirrors the entire surface with a promise-returning `question()` and adds `readlinePromises.Readline`, a batched-cursor-actions class (`cursorTo`/`moveCursor`/`clearLine`/`clearScreenDown` queued, then flushed via `commit()`/discarded via `rollback()`). Because line editing needs raw-mode terminal input and cursor/width math, this module is functionally dependent on `node:tty` (raw mode, `isTTY`, terminal size) and `node:events` (`Interface` extends `EventEmitter`).

## 2. Exported API surface (COMPLETE)

### Classes

#### `readline.InterfaceConstructor` (abstract base, extends `EventEmitter`)

Not directly exported/constructed by user code — both `readline.Interface` and `readlinePromises.Interface` extend it and share every member below. Documented once here to avoid duplication.

**Instance properties**

| Property | Type | Notes |
|---|---|---|
| `rl.line` | `string` | Current input buffer being edited. Always a string (never `undefined`, since v15.8.0/v14.18.0). Reset to `''` immediately after the `'line'` event is emitted. Mutating it directly without also updating `rl.cursor` desyncs the rendered line — discouraged outside of read-only inspection. |
| `rl.cursor` | `number \| undefined` | Cursor position as an offset into `rl.line`. Determines which portion of the line is edited/redrawn next and where the terminal caret is painted. |

**Instance methods**

| Signature | Returns | Notes |
|---|---|---|
| `rl.close()` | `void` | Relinquishes control of `input`/`output`, stops the instance from processing further input, emits `'close'`. Idempotent-ish in practice (Node does not document double-close as an error, but no further events fire). |
| `rl[Symbol.dispose]()` | `void` | Alias of `rl.close()`. Added v23.10.0/v22.15.0 — lets `using rl = createInterface(...)` auto-close. |
| `rl.pause()` | `void` | Pauses the `input` stream (can be resumed later). Emits `'pause'`. |
| `rl.resume()` | `void` | Resumes a paused `input` stream. Emits `'resume'`. |
| `rl.prompt([preserveCursor])` | `void` | Writes the configured prompt string to `output` on a new line. `preserveCursor: boolean` (default `false`) — if `true`, does not reset cursor column to `0`. Resumes `input` if paused. No-op if `output` is `null`/`undefined`. |
| `rl.setPrompt(prompt: string)` | `void` | Sets the string written by `rl.prompt()`. |
| `rl.getPrompt()` | `string` | Returns the current prompt string (added v15.3.0/v14.17.0). |
| `rl.write(data: string, key?: Key)` | `void` | Writes `data` to `output`; if `key` is given, `data` is ignored and the key sequence is written instead (only meaningful when `output` is a TTY). Resumes `input` if paused. No-op if `output` is `null`/`undefined`. Example: `rl.write(null, { ctrl: true, name: 'u' })` emulates Ctrl+U. |
| `rl.getCursorPos()` | `CursorPos` (`{ rows: number, cols: number }`) | Real on-screen cursor position accounting for prompt length, line wrapping, and multi-line prompts (added v13.5.0/v12.16.0). |
| `rl[Symbol.asyncIterator]()` | `AsyncIterableIterator<string>` | Lets `for await (const line of rl)` consume lines; calls `rl.close()` when the loop ends (`break`, `return`, or the `input` stream ending). Stabilized (no longer experimental) v11.14.0/v10.17.0. |

**Events** (emitted by both `readline.Interface` and `readlinePromises.Interface` instances — see full table in "Events" below)

`'close'`, `'error'`, `'line'`, `'history'`, `'pause'`, `'resume'`, `'SIGCONT'`, `'SIGINT'`, `'SIGTSTP'`.

#### `readline.Interface extends InterfaceConstructor`

Callback-based `Interface`. Constructed via `readline.createInterface(options)` (canonical) — a legacy positional constructor form `new readline.Interface(input[, output[, completer[, terminal]]])` also exists for backward compatibility but `createInterface(options)` is the form RTS targets; the positional form is noted here only so a ported program that uses it isn't silently mis-parsed.

**Additional instance method (beyond `InterfaceConstructor`)**

| Signature | Returns | Variant | Notes |
|---|---|---|---|
| `rl.question(query: string, callback: (answer: string) => void): void` | `void` | callback | Writes `query` to `output`, waits for one line of `input`, invokes `callback(answer)`. Resumes `input` if paused. No-op if `output` is `null`/`undefined`. Throws if called after `rl.close()` (see §4). |
| `rl.question(query: string, options: QuestionOptions, callback: (answer: string) => void): void` | `void` | callback | `options.signal: AbortSignal` lets the pending question be canceled; on abort, the callback is never invoked (Node cancels the underlying one-shot `'line'` listener). |

#### `readlinePromises.Interface extends InterfaceConstructor`

Promise-based `Interface` (`node:readline/promises`, added v17.0.0). Identical to `readline.Interface` except:

| Signature | Returns | Variant | Notes |
|---|---|---|---|
| `rl.question(query: string, options?: QuestionOptions): Promise<string>` | `Promise<string>` | promise | Resolves with the answer on the next `'line'`. Rejects with the abort reason if `options.signal` aborts. Returns an already-rejected promise if called after `rl.close()`. |

#### `readlinePromises.Readline`

TTY cursor-manipulation class with a queued-actions + explicit-flush model (added v17.0.0). Not an `EventEmitter`; no events.

```ts
new readlinePromises.Readline(stream: NodeJS.WritableStream, options?: ReadlineOptions)
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `stream` | `stream.Writable` (must be a TTY) | no | — |
| `options.autoCommit` | `boolean` | yes | `false` |

**Instance methods** (all but `commit()` synchronously queue an action and return `this` for chaining; nothing is written to `stream` until `commit()` — unless `autoCommit: true`, in which case each call also implicitly commits)

| Signature | Returns | Notes |
|---|---|---|
| `rl.clearLine(dir: Direction)` | `this` | Queues "clear current line": `-1` left-of-cursor, `1` right-of-cursor, `0` entire line. |
| `rl.clearScreenDown()` | `this` | Queues "clear from cursor position downward". |
| `rl.cursorTo(x: number, y?: number)` | `this` | Queues "move cursor to absolute `(x, y)`" (`y` omitted → only column changes). |
| `rl.moveCursor(dx: number, dy: number)` | `this` | Queues "move cursor by relative `(dx, dy)`". |
| `rl.commit()` | `Promise<void>` | Sends every queued action to `stream` in order, then clears the queue. |
| `rl.rollback()` | `this` | Discards every queued action without writing anything. |

### Top-level functions

| Function | Variant |
|---|---|
| `readline.createInterface(options: ReadLineOptions)` | sync (factory) |
| `readlinePromises.createInterface(options: ReadLineOptions)` | sync (factory) |
| `readline.clearLine(stream, dir[, callback])` | callback |
| `readline.clearScreenDown(stream[, callback])` | callback |
| `readline.cursorTo(stream, x[, y][, callback])` | callback |
| `readline.moveCursor(stream, dx, dy[, callback])` | callback |
| `readline.emitKeypressEvents(stream[, interface])` | sync |

#### `readline.createInterface(options)`

Added v0.1.98. History: `signal` option (v15.14.0/v14.18.0), `history` option (v15.8.0/v14.18.0), `tabSize` option (v13.9.0), `crlfDelay` max-limit removed (v8.3.0/v6.11.4), `crlfDelay` option (v6.6.0), `prompt` option (v6.3.0), `historySize` can be `0` (v6.0.0).

| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `ReadLineOptions` | no | — |

Returns: `readline.Interface`. Variant: sync (factory function; the interface itself then does async line-by-line I/O). Throws: `TypeError`/`ERR_INVALID_ARG_TYPE` if `options.input` is missing or not a `Readable`.

#### `readlinePromises.createInterface(options)`

Added v17.0.0. Same `options` shape as `readline.createInterface`, except the documented default for `escapeCodeTimeout` is stated independently (`500`, same value). Returns: `readlinePromises.Interface`. Variant: sync (factory).

#### `readline.clearLine(stream, dir[, callback])`

Added v0.7.7. History: v18.0.0 invalid callback → `ERR_INVALID_ARG_TYPE` (was `ERR_INVALID_CALLBACK`); v12.7.0 stream `write()` callback/return value exposed.

| Param | Type | Optional | Default |
|---|---|---|---|
| `stream` | `stream.Writable` (TTY) | no | — |
| `dir` | `Direction` (`-1 \| 0 \| 1`) | no | — |
| `callback` | `() => void` | yes | — |

Returns: `boolean` — `false` if the caller should wait for `'drain'` on `stream` before writing more, `true` otherwise. Variant: callback (the callback param is optional; without it this behaves synchronously from the caller's perspective). Throws: `ERR_INVALID_ARG_TYPE` for a non-function `callback`.

#### `readline.clearScreenDown(stream[, callback])`

Added v0.7.7 (same history notes as `clearLine`).

| Param | Type | Optional |
|---|---|---|
| `stream` | `stream.Writable` (TTY) | no |
| `callback` | `() => void` | yes |

Returns: `boolean`. Variant: callback. Throws: `ERR_INVALID_ARG_TYPE`.

#### `readline.cursorTo(stream, x[, y][, callback])`

Added v0.7.7 (same history notes).

| Param | Type | Optional |
|---|---|---|
| `stream` | `stream.Writable` (TTY) | no |
| `x` | `number` | no |
| `y` | `number` | yes |
| `callback` | `() => void` | yes |

Returns: `boolean`. Variant: callback. Throws: `ERR_INVALID_ARG_TYPE`; `ERR_INVALID_CURSOR_POS` (verify — Node's internal cursor-math utility throws this when `y` is supplied without a valid `x`, or either is `NaN`).

#### `readline.moveCursor(stream, dx, dy[, callback])`

Added v0.7.7 (same history notes).

| Param | Type | Optional |
|---|---|---|
| `stream` | `stream.Writable` (TTY) | no |
| `dx` | `number` | no |
| `dy` | `number` | no |
| `callback` | `() => void` | yes |

Returns: `boolean`. Variant: callback. Throws: `ERR_INVALID_ARG_TYPE`.

#### `readline.emitKeypressEvents(stream[, interface])`

Added v0.7.7.

| Param | Type | Optional |
|---|---|---|
| `stream` | `stream.Readable` | no |
| `interface` | `readline.InterfaceConstructor` | yes |

Returns: `void`. Variant: sync (installs a listener; the listener itself then fires `'keypress'` events asynchronously as bytes arrive). Description: makes `stream` begin emitting `'keypress'` for every received input chunk; if `stream` is a TTY it must already be in raw mode (`stream.setRawMode(true)`) for per-keystroke events instead of per-line/buffered ones. Called automatically by every `readline.Interface`/`readlinePromises.Interface` on its own `input` when that `input` is a terminal. Closing the readline instance does **not** stop `input` from continuing to emit `'keypress'`. The optional `interface` argument, when passed, disables autocompletion behavior for that interface specifically when pasted (multi-character, no-delay) input is detected, to avoid triggering Tab-completion mid-paste.

### Properties & constants

No module-level constants or enums are exported. `Direction` (`-1 | 0 | 1`) and the `Key` shape are documented type conventions only (§3) — there is no `readline.Direction` runtime value to import.

### Events

| Event | Emitted on | Listener signature | Notes |
|---|---|---|---|
| `'close'` | `Interface` | `() => void` | Fires when `rl.close()` runs, `input` emits `'end'`, `input` receives Ctrl+D (EOT) on an empty line, or `input` receives Ctrl+C (SIGINT) with no `'SIGINT'` listener registered. No further events fire on this interface afterward. |
| `'error'` | `Interface` | `(error: Error) => void` | Added v16.0.0. Fires when `input` emits `'error'`. |
| `'line'` | `Interface` | `(input: string) => void` | Fires when `input` receives an end-of-line (`\n`, `\r`, or `\r\n` per `crlfDelay`), typically Enter/Return; also fires for Ctrl+D on non-empty input, and for pasted content containing an embedded newline. |
| `'history'` | `Interface` | `(history: string[]) => void` | Added v15.8.0/v14.18.0. Fires on every history mutation (additions capped by `historySize`, dedup by `removeHistoryDuplicates`); `history[0]` is the most recent entry. |
| `'pause'` | `Interface` | `() => void` | Fires when `input` is paused, or on `'SIGCONT'`. |
| `'resume'` | `Interface` | `() => void` | Fires when `input` is resumed. |
| `'SIGCONT'` | `Interface` | `() => void` | POSIX only (no-op source on Windows). Fires when the process is foregrounded again (`fg` after Ctrl+Z). |
| `'SIGINT'` | `Interface` | `() => void` | Fires on Ctrl+C. If no listener is registered, Node's default behavior is to emit `'pause'` instead of closing. |
| `'SIGTSTP'` | `Interface` | `() => void` | POSIX only (no-op source on Windows). Fires on Ctrl+Z; if unhandled, the default action backgrounds the process. |
| `'keypress'` | the **input stream** itself (not the `Interface`) — installed via `readline.emitKeypressEvents()`, which every terminal-mode `Interface` calls on its own `input` | `(str: string \| undefined, key: Key) => void` | `str` is the printable character (if any) decoded from the byte sequence, or `undefined` for a non-printable/control sequence; `key` is the structured key descriptor (§3). Not an `Interface` event — subscribe on the stream (`process.stdin.on('keypress', ...)`), independent of any `Interface` bound to that stream. |

## 3. Types & option objects

```ts
/** Options accepted by readline.createInterface() / readlinePromises.createInterface(). */
interface ReadLineOptions {
  /** The Readable stream to read input from. Required. */
  input: NodeJS.ReadableStream;
  /** The Writable stream to write prompts/output to. */
  output?: NodeJS.WritableStream;
  /** Tab-completion hook. See Completer / AsyncCompleter below. */
  completer?: Completer | AsyncCompleter;
  /** Treat input/output as a TTY (ANSI/VT100 escapes). Default: output.isTTY at construction time. */
  terminal?: boolean;
  /** Initial history entries, most-recent-first. Only used when terminal is true. Default: []. */
  history?: string[];
  /** Max retained history lines; 0 disables history. Only used when terminal is true. Default: 30. */
  historySize?: number;
  /** Drop an older duplicate when a new line matches an existing history entry. Default: false. */
  removeHistoryDuplicates?: boolean;
  /** String written by rl.prompt(). Default: '> '. */
  prompt?: string;
  /** Max ms between '\r' and '\n' to still treat as one line break; coerced to a minimum of 100. Use Infinity to always treat '\r\n' as one line break. Default: 100. */
  crlfDelay?: number;
  /** Ms to wait for further bytes when an ambiguous escape sequence is being read. Default: 500. */
  escapeCodeTimeout?: number;
  /** Number of spaces one tab character occupies (minimum 1). Default: 8. */
  tabSize?: number;
  /** Abort this signal to close the interface (equivalent to calling rl.close()). */
  signal?: AbortSignal;
}

/** Options accepted by rl.question() (both callback and promise Interface). */
interface QuestionOptions {
  /** Cancels the pending question. Callback variant: callback is never invoked. Promise variant: promise rejects with the abort reason. */
  signal?: AbortSignal;
}

/** Options accepted by new readlinePromises.Readline(stream, options). */
interface ReadlineOptions {
  /** If true, each queued action commits immediately instead of waiting for an explicit rl.commit(). Default: false. */
  autoCommit?: boolean;
}

/** Direction argument for clearLine (both module-level and Readline#clearLine) and cursorTo's y-only edge case. */
type Direction = -1 | 0 | 1;

/** rl.getCursorPos() return shape. */
interface CursorPos {
  rows: number;
  cols: number;
}

/** Key-sequence object accepted by rl.write(data, key) and delivered by the 'keypress' event. */
interface Key {
  sequence?: string;   // (verify) raw escape sequence / bytes that produced this key; present on 'keypress' events, not required on rl.write() input
  name?: string;       // symbolic key name, e.g. 'return', 'up', 'a', 'tab'
  ctrl?: boolean;      // default false
  meta?: boolean;      // default false
  shift?: boolean;     // default false
  code?: string;       // (verify) raw terminal escape code identifier for special keys (e.g. '[A' for Up) — mentioned in Node's internal implementation, not spelled out in the public docs fetched for this spec
}

/** Sync completer: takes the current line, returns [matches, substringUsedForMatching]. */
type CompleterResult = [string[], string];
type Completer = (line: string) => CompleterResult;

/** Async completer: either callback-style or Promise-returning. */
type AsyncCompleter =
  | ((line: string, callback: (err: Error | null, result: CompleterResult) => void) => void)
  | ((line: string) => Promise<CompleterResult>);
```

## 4. Node semantics & edge cases

- **`terminal` detection and behavior split.** `terminal` defaults to `output.isTTY` at construction time (not re-checked later). When `terminal` is `true`: history, line editing (cursor movement, kill/yank, Ctrl+N/Ctrl+P history nav), Tab completion, and raw-mode `'keypress'` parsing are all active; when `false` (piped/file input): the interface is a pure line-splitter — bytes are decoded and split on `\n`/`\r`/`\r\n` (per `crlfDelay`) into `'line'` events, no history, no cursor math, no completer invocation, `rl.write(data, key)`'s `key` form is meaningless (only supported when `output` is a TTY).
- **`crlfDelay` semantics.** Guards against a `\r` and its paired `\n` arriving in separate stream chunks and being double-counted as two line breaks; the delay is coerced to a minimum of `100`ms; `Infinity` makes every `\r\n` pair always count as a single line break regardless of arrival timing — the documented, recommended setting when reading a file known to use CRLF line endings (`crlfDelay: Infinity`).
- **History.** Only active when `terminal: true`. `historySize: 0` disables history entirely (allowed since v6.0.0). `removeHistoryDuplicates: true` removes an older identical entry instead of allowing both to coexist. `'history'` fires on every mutating change, letting a program persist/restore history across runs.
- **`rl.line` type stability.** Guaranteed to always be a `string` (never `undefined`) since v15.8.0/v14.18.0 — earlier versions could observe `undefined` before the first keystroke; RTS's implementation should initialize to `''` from construction, matching current Node.
- **Windows vs POSIX.** `'SIGCONT'` and `'SIGTSTP'` are POSIX-only signals — they are never emitted on Windows (no background/foreground job control). Several line-editing keybindings differ by platform (from the Node keybinding table): Ctrl+Shift+Backspace (delete line left) doesn't work on Linux/Mac/Windows (effectively inert everywhere in Node's own implementation); Ctrl+Shift+Delete (delete line right) doesn't work on Mac; Ctrl+D (delete right / close-if-empty-line) doesn't work on Windows; Ctrl+Z (background the process, POSIX job control) doesn't work on Windows; Ctrl+Backspace (delete word left) doesn't work on Linux/Mac/Windows; Ctrl+Delete (delete word right) doesn't work on Mac; Ctrl+Left arrow (word left) doesn't work on Mac; Ctrl+Right arrow (word right) doesn't work on Mac; Meta+Delete (delete word right, alt binding) doesn't work on Windows; Meta+Backspace (delete word left, alt binding) doesn't work on Mac. RTS must reproduce this exact platform matrix rather than assuming keys behave uniformly.
- **Full keybinding table** (POSIX/xterm-style raw-mode line editing, all bindings RTS's `Interface` terminal-mode implementation must support): Ctrl+C → emit `'SIGINT'` or close if unhandled; Ctrl+H → delete left; Ctrl+D → delete right, or close if line is empty (EOF); Ctrl+U → delete from cursor to line start; Ctrl+K → delete from cursor to line end; Ctrl+Y → yank last Ctrl+U/Ctrl+K-deleted text; Meta+Y → cycle among previously deleted texts (only right after Ctrl+Y/Meta+Y); Ctrl+A → start of line; Ctrl+E → end of line; Ctrl+B → back one char; Ctrl+F → forward one char; Ctrl+L → clear screen; Ctrl+N → next history item; Ctrl+P → previous history item; Ctrl+\_ (key code `0x1F`) → undo; Ctrl+6 (key code `0x1E`) → redo; Ctrl+Z → background process (POSIX); Ctrl+W/Ctrl+Backspace → delete word left; Ctrl+Delete → delete word right; Ctrl+Left / Meta+B → word left; Ctrl+Right / Meta+F → word right; Meta+D / Meta+Delete → delete word right; Meta+Backspace → delete word left.
- **`process.stdin` keeps the process alive.** An `Interface` over `stdin` prevents process exit until EOF (Ctrl+D) is received; the documented escape hatch is `process.stdin.unref()` to allow exit while still consuming input if it arrives.
- **TTY output compatibility.** For best results in terminal mode, `output` should expose a `.columns` property and emit `'resize'` when the terminal is resized (`process.stdout` does this automatically when it is a TTY) — `Interface` uses `.columns` for line-wrap math in `rl.getCursorPos()` and redraw-on-resize.
- **`rl.question()` after `close()`.** Callback variant: calling `question()` on a closed interface is documented to throw/error (exact code unverified from the fetched docs — flagged `ERR_USE_AFTER_CLOSE` pending source verification, §7). Promise variant: returns an already-rejected `Promise` rather than throwing synchronously — a deliberate asymmetry between the two APIs that RTS must preserve (do not "fix" the callback API to also silently reject).
- **`AbortSignal` on `question()`.** Passing `options.signal` lets the pending question be canceled from outside; the callback variant's callback is simply never invoked (no error surfaces through the `question()` call itself — the caller must listen on `signal.addEventListener('abort', ...)` if it wants to react); the promise variant's promise rejects with the signal's abort reason.
- **`ERR_INVALID_ARG_TYPE`.** Thrown synchronously by every function in this module (both classes and top-level utilities) when passed a malformed callback (v18.0.0 unified this — previously some paths threw the separate `ERR_INVALID_CALLBACK`) or a non-stream `input`/`stream` argument.
- **`ERR_INVALID_CURSOR_POS`** (verify) — believed to originate from the internal cursor-math helper shared by `cursorTo`/`getCursorPos`/wrap calculations when given a `NaN` or otherwise invalid coordinate; not spelled out in the fetched public docs, so RTS should verify the exact trigger condition and message against Node's own source/tests before finalizing error parity.
- **Deprecations.** No deprecated readline-specific APIs as of Node 25; the module surface has been stable since the promises variant landed in v17.0.0 (aside from ongoing minor additive history entries in the option table above).
- **Security notes.** `node:readline` performs no filesystem/network/process I/O of its own — it operates purely on whatever `Readable`/`Writable` streams the caller supplies (which themselves may be subject to fs/net permission-model checks upstream). One practical hazard worth documenting for RTS users: writing untrusted data to a TTY `output` via `rl.write()`/`console.log`-style output can inject terminal escape sequences (cursor moves, color changes, in rare terminal-emulator bugs even more) — this is a general terminal-security concern, not something `node:readline` itself mitigates or that Node's docs flag as an in-scope security note.

## 5. RTS implementation notes

### 5.1 Native impl mapping

The overwhelming majority of `node:readline`'s logic — line-buffer state machine, `crlfDelay` timing, history array management, cursor-offset arithmetic, the terminal escape-sequence-to-`Key`-object parser, and the full keybinding table in §4 — is pure ECMA-level string/array manipulation with no OS dependency of its own. This matches real Node's own implementation (`lib/internal/readline/*.js` is almost entirely JS; the native pieces it leans on live in the separate `tty` binding). Concretely, `node:readline` itself needs **no new Rust std module**; instead it composes two sibling `rts-node` modules that must exist first:

- **`node:tty`** (prerequisite, out of scope for this spec) — owns raw-mode toggling (`stream.setRawMode(bool)`, POSIX `termios`/`cfmakeraw` + `tcsetattr` vs Windows Console API `SetConsoleMode`/`ENABLE_VIRTUAL_TERMINAL_INPUT`), `isTTY` detection, and terminal column/row size (`ioctl(TIOCGWINSZ)` / `GetConsoleScreenBufferInfo`) plus the `'resize'` event on `process.stdout`.
- **`node:process`** (prerequisite, out of scope for this spec) — owns the actual byte-level `stdin` read / `stdout` write plumbing that `input`/`output` streams wrap, and POSIX signal delivery (`SIGINT`/`SIGCONT`/`SIGTSTP`) that `'SIGINT'`/`'SIGCONT'`/`'SIGTSTP'` events forward.
- **`node:events`** (prerequisite) — `InterfaceConstructor` extends the `.ts` `EventEmitter` from `node:events` (see `docs/node-implementation/events.md`); no separate emitter implementation should exist inside `readline`.

The one place a native Rust primitive is worth considering (not strictly required for a correct, if approximate, P1 implementation) is **Unicode display-width measurement** for cursor math with wide characters (CJK, emoji, combining marks): Node's own `getStringWidth()` uses a bundled East-Asian-width data table, which is impractical to hand-maintain accurately in `.ts`. RTS should back this with the `unicode-width` Rust crate inside `rts-node` (see §5.2) rather than a hand-rolled `.ts` table; a first P1 pass may instead approximate with `[...str].length` (codepoint count) and revisit if CJK-heavy terminal fixtures show visibly wrong wrapping (tracked in §7).

### 5.2 ABI surface

`node:readline` itself proposes **at most one** new native `extern "C"` symbol; everything else it needs is *consumed from*, not defined by, this module (owned by `node:tty`/`node:process`/`node:events`, flagged again in §5.7 so the dependency isn't lost).

| Symbol | Args (`AbiType`) | Return (`AbiType`) | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_READLINE_STRING_WIDTH` | `StrPtr` | `I32` | Optional. Display-column width of a UTF-8 string via the `unicode-width` crate, matching Node's internal `getStringWidth()` closely enough for correct multi-line prompt wrap math. Deferred to a codepoint-count `.ts` approximation for the first P1 pass (§5.1/§7); add this native fn only if a fixture demonstrates the approximation is visibly wrong. |

Everything else in `node:readline`'s surface is **zero new ABI**:

- `NodespaceSpec` entries (mirroring the `fs`/`os`/`process` pattern in `crates/rts-node/src/lib.rs`), one per subpath:

  ```rust
  pub const SPEC: NodespaceSpec = NodespaceSpec {
      node_module: "readline",
      ns_prefix: "node_readline",
      members: &[], // or &[STRING_WIDTH] if the optional native fn above is added
  };

  pub const PROMISES_SPEC: NodespaceSpec = NodespaceSpec {
      node_module: "readline/promises",
      ns_prefix: "node_readline_promises",
      members: &[],
  };
  ```

  `ns_prefix_for("node:readline")` → `Some("node_readline")` and `ns_prefix_for("node:readline/promises")` → `Some("node_readline_promises")` are what let the module loader mount the two `.ts` shims (`crates/rts-node/src/readline/readline.ts`, `crates/rts-node/src/readline/readline_promises.ts`) under their respective specifiers — the same "empty-members-table, `.ts`-shim-only" shape `node:events` already established (see `events.md` §5.2 for the precedent).
- **Handles:** none. `Interface` and `readlinePromises.Readline` instances are ordinary GC'd JS objects (shape-based hidden classes) holding references to their `input`/`output` stream objects (themselves `node:stream`/`node:tty`/`node:process` values) — no Rust-side resource is owned directly by `readline`.
- **`.ts` shim vs native extern split:** effectively 100% `.ts` shim, 0–1 native externs (the optional string-width helper). All byte I/O, raw-mode toggling, and signal delivery are delegated to `node:tty`/`node:process` calls made *from* the `.ts` shim, not reimplemented here.

### 5.3 Async model

- **Non-TTY line splitting** (`terminal: false`): `Interface` attaches a listener to `input`'s existing data-delivery mechanism (however `node:stream`/`node:fs`/`node:process` model it — callback-driven `'data'` events per the Node stream contract) and runs the line-splitting state machine synchronously inside that callback, emitting `'line'` synchronously per completed line. No tokio/event-loop hop is introduced by `readline` itself — it inherits whatever async model the underlying stream uses.
- **TTY raw-mode keypress path** (`terminal: true`): `emitKeypressEvents()` similarly attaches to `input`'s raw byte delivery and runs the escape-sequence parser synchronously per chunk, emitting `'keypress'` (on the stream) and driving the `Interface`'s line-editing state machine (which then emits `'line'` when Enter is detected) — still single-threaded/cooperative, no tokio dependency intrinsic to parsing.
- **`rl.question()` (callback variant):** registers a one-shot `'line'` listener (conceptually `rl.once('line', callback)` after writing the query) — pure `.ts`-level `EventEmitter` mechanics, no Promise involved.
- **`rl.question()` (promises variant) / `rl[Symbol.asyncIterator]()`:** implemented as a manual `Promise` executor wrapping the same one-shot-`'line'`-listener pattern (directly analogous to `events.once()` in `events.md` §5.1/§5.3) — resolved/settled through the engine's microtask queue, not `tokio::spawn_blocking`.
- **`AbortSignal` cancellation** (`question({signal})`): reuses the already-specified ambient `AbortSignal`/`AbortController` (`rts-shared/src/stdlib/events.ts` per `events.md` §5.3) — registers an abort listener that either skips invoking the callback (callback variant) or rejects the pending promise (promises variant); no cross-thread signal involved.
- **`readlinePromises.Readline.commit()`:** returns a `Promise<void>` that resolves once the queued escape sequences have been written to `stream`. Whether this can resolve synchronously-via-microtask (if the underlying `node:tty`/`node:stream` write is effectively synchronous/buffered) or must wait for a real write-completion callback depends on `node:stream`'s own write design — flagged as a soft dependency in §5.7; `readline` itself does not introduce new async machinery beyond wrapping that single write call in a `Promise`.
- **No tokio / shared-runtime need intrinsic to this module.** Everything above is single-threaded, cooperative, microtask-driven logic; any actual blocking I/O (reading stdin bytes, writing stdout bytes, raw-mode syscalls) is `node:tty`/`node:process`'s responsibility, not `readline`'s.

### 5.4 Multithread / worker interaction

- An `Interface`/`Readline` instance is ordinary **per-thread-region heap data** under `docs/specs/rts-threading-model.md` — `threadLocal` by construction, exactly like `node:events`' `EventEmitter` (per `events.md` §5.4). Nothing in this module's public surface needs `shared`/promotion-on-publication semantics.
- **`process.stdin` is a de facto process-global singleton resource in real Node**, and worker threads do not inherit it: a `worker_threads.Worker` gets `stdin: null` by default (a separate, optional stream only if the parent explicitly wires one via `stdin: true` in `Worker` options), never the *same* stream object as the main thread. RTS must preserve this: only the thread that owns `process.stdin` (conventionally the main thread) should construct an `Interface` bound to it. A worker thread wanting interactive line input should read from its own dedicated stream (a file, a `MessagePort`-relayed channel) rather than reaching for a shared `process.stdin`.
- **Raw-mode is a genuine single-OS-resource hazard.** There is exactly one controlling terminal / one stdin file descriptor per process; if two independent `Interface`s (even on the same thread, let alone different threads) both call `setRawMode(true)`/`setRawMode(false)` on it, their toggles race. Real Node does not solve this either — it implicitly assumes one readline consumer of `stdin` per process. RTS's spec position: document this as a known footgun (matching upstream), not something `node:readline` itself needs to arbitrate with a lock; do not add novel cross-thread coordination that Node's own semantics don't have.
- No `SharedArrayBuffer`/shared-heap involvement anywhere in this module — there is no byte buffer in the public surface at all (§5.5).

### 5.5 Buffer / TypedArray interop

None at the `readline` API boundary. `rl.line`, the `'line'` event payload, `rl.question()`'s answer, and `Completer`/`AsyncCompleter` results are always **decoded JS strings** — `node:readline` operates purely at the text level; any raw-byte decoding (respecting the input stream's configured encoding, default UTF-8) happens one layer down, inside whatever `node:stream`/`node:fs`/`node:process`/`node:net` implementation produced `input`. `rl.write(data, key)` also only ever accepts a `string` for `data`. If a caller pipes a stream that itself emits `Buffer` chunks without a string encoding configured, that is a `node:stream` concern (readline would need the stream to be in "string mode") — out of scope for this spec.

### 5.6 Doctrine placement

- `node:readline` is **entirely non-primordial**: `Interface`/`InterfaceConstructor`/`Readline` have no native literal/syntactic form — the engine must never hardcode any of these class names, exactly like `node:events`' `EventEmitter`/`EventTarget`.
- **Resolution path:** `import { createInterface } from "node:readline"` → specifier stripped of the `"node:"` prefix → looked up via `ns_prefix_for("node:readline")` in `rts-node`'s `NODE_SPECS` table (`crates/rts-node/src/lib.rs`) → `Some("node_readline")`; `import ... from "node:readline/promises"` resolves through a **separate** `NODE_SPECS` entry keyed on the full `"readline/promises"` module string (the same pattern other dual-surface Node modules with a `/promises` sibling use elsewhere in `rts-node`, e.g. `node:fs`/`node:fs/promises`, `node:dns`/`node:dns/promises`) → `Some("node_readline_promises")`. Because both `SPEC.members` tables are empty (or hold only the one optional string-width helper), the *only* job this data-table resolution does is let the module loader mount the corresponding `.ts` shim under the right specifier — `node_lookup()` never needs to resolve a member for the vast majority of this module's calls.
- **Native-extern vs `.ts`-shim split:** ~100% `.ts` shim (`crates/rts-node/src/readline/readline.ts` + `readline_promises.ts`), with at most one native extern (`__RTS_FN_NODE_READLINE_STRING_WIDTH`, §5.2) if pursued. `InterfaceConstructor` is a `.ts` class extending `node:events`' `.ts` `EventEmitter` (a same-crate, in-family `.ts`-to-`.ts` dependency — no Rust-level crate coupling), consistent with the "no native syntax ⇒ `.ts` shim" doctrine bucket `node:events`/`Map`/`Set` already occupy.

### 5.7 Shared-infra dependencies (FLAG)

- **Promise/microtask draining loop.** `rl.question()` (promises variant), `rl[Symbol.asyncIterator]()`, and `readlinePromises.Readline.commit()` all need the engine's microtask queue to actually drain pending `.then()`/`Promise` executor callbacks. As in `events.md` §5.7, this primitive currently lives under `rts-std` (`runtime/async_rt.rs`, the `promise` namespace); since `rts-node` cannot depend on `rts-std`, it must be reachable from a crate `rts-node` can depend on (hoisted into `rts-engine` or a shared low crate). This module needs no `tokio` (no blocking I/O of its own), only microtask scheduling/draining.
- **`node:tty` (in-family, sibling `rts-node` module, not yet specified).** Raw-mode toggling, `isTTY`, terminal column/row size, and `'resize'` delivery are hard prerequisites for the entire terminal-mode (`terminal: true`) code path — non-TTY line splitting can ship without it, but interactive CLI use (history, arrow-key navigation, Tab completion, Ctrl+C/Ctrl+D handling) cannot. Flagged as a blocking sibling dependency, not a doctrine violation (both live under `rts-node`).
- **`node:process` (in-family, sibling `rts-node` module, not yet specified).** Owns the actual `stdin`/`stdout` byte I/O plumbing `input`/`output` streams wrap, plus POSIX signal delivery (`SIGINT`/`SIGCONT`/`SIGTSTP`) that this module's same-named events forward. Same "blocking sibling, in-family" flag as `node:tty`.
- **`node:events` (in-family, sibling `rts-node` module — spec already written, see `docs/node-implementation/events.md`).** `InterfaceConstructor` extends its `.ts` `EventEmitter`; `readline` should not implement a second emitter.
- **Ambient `AbortSignal`/`AbortController`** (`rts-shared/src/stdlib/events.ts`, already flagged as landed/available per `events.md`). Needed for `question({signal})` cancellation and the `signal` option on `createInterface()`.
- **`unicode-width` crate (new, self-contained within `rts-node`, not a hoist need).** Only if the optional `__RTS_FN_NODE_READLINE_STRING_WIDTH` native fn (§5.2) is implemented; this is a fresh `rts-node`-owned Cargo dependency, not something to find/reuse from `rts-std` (no equivalent helper is known to exist there for column-accurate Unicode width).
- **No dependency** on `fs`, `net`, `tls`, `crypto`, or the shared `tokio` runtime beyond the microtask-drain primitive noted above.

### 5.8 Implementation phases

(a) Scaffold `node_readline`/`node_readline_promises` `NodespaceSpec` entries (empty `members`), registered in `NODE_SPECS` — makes both specifiers resolve to their (initially empty) `.ts` shims.

(b) Implement `InterfaceConstructor` as a `.ts` class extending `node:events`' `EventEmitter`: `line`/`cursor` state, `close`/`[Symbol.dispose]`/`pause`/`resume`/`setPrompt`/`getPrompt` — no TTY/history/completion logic yet.

(c) Implement the **non-TTY line-splitting path** first (`terminal: false`): byte→string decode (delegated to the underlying stream), split on `\n`/`\r`/`\r\n` per `crlfDelay`, emit `'line'`/`'close'`. This unblocks the most common use case (reading a file/piped stream line by line) without touching raw mode, cursor math, or ANSI parsing at all.

(d) Implement `readline.createInterface()` + `rl.write()` (string form only — no `key` form yet) + `rl.prompt()`/`rl.getCursorPos()` stubs valid for the non-TTY path.

(e) Implement `rl[Symbol.asyncIterator]()` (for-await consumption), reusing the same manual-`Promise`-executor pattern as `events.once()` (`events.md` §5.1).

(f) Implement `readline.Interface.question(query[, options], callback)` (callback variant) including `AbortSignal` cancellation.

(g) Implement `readlinePromises.Interface` (mirrors (b)–(f), `question()` returns a `Promise`) and `readlinePromises.createInterface()`.

(h) Implement the **TTY/raw-mode path**, gated on `node:tty` existing: `emitKeypressEvents(stream[, interface])`'s ANSI escape-sequence parser (arrow keys, Ctrl/Alt/Meta combinations, function keys) emitting `'keypress'` `(str, key)` on the input stream; wire terminal-mode `Interface` construction to consume these events and implement the full line-editing keybinding table from §4 (cursor movement, kill/yank, history navigation).

(i) Implement `completer` support: sync `(line) => [matches, substring]`, async callback `(line, cb)`, and `Promise`-returning variants; wire Tab-triggered completion display in terminal mode.

(j) Implement `readline.clearLine`/`clearScreenDown`/`cursorTo`/`moveCursor` (module-level utility functions writing raw ANSI escapes to any writable TTY stream) and `readlinePromises.Readline` (pending-actions queue + `commit()`/`rollback()`/`autoCommit`).

(k) Wire `'SIGINT'`/`'SIGCONT'`/`'SIGTSTP'` (POSIX signal delivery via `node:process` → forwarded as `Interface` events; `'SIGCONT'`/`'SIGTSTP'` are no-op sources on Windows per §4).

(l) (Optional, only if a fixture shows visibly wrong line-wrap with CJK/emoji) add `__RTS_FN_NODE_READLINE_STRING_WIDTH` via `unicode-width` and switch cursor-math from the codepoint-count approximation to it.

(m) Cross-runtime fixtures (§6); wire into the existing cross-runtime harness.

## 6. Test plan

`tests/node-readline/*.test.ts` (standard `rts:test` `describe`/`test`/`expect` template; interactive-TTY-only behaviors use a minimal fake `Writable`/`Readable` pair that reports `isTTY: true` and captures written bytes, since CI has no real terminal):

1. **Non-TTY line splitting** — feed a mock `Readable` with `"a\nb\r\nc\rd"` and `crlfDelay: Infinity`; assert exactly the lines `["a", "b", "c\rd"]` or the correctly-CRLF-normalized equivalent are emitted via `'line'`, and `'close'` fires once the source ends.
2. **`readline.createInterface` + callback `rl.question`** — write a query, feed one line of input, assert the callback receives the trimmed-of-newline answer; assert `output` received the query text before the answer arrived.
3. **`readlinePromises.createInterface` + `await rl.question()`** — same as (2) but awaited; assert it resolves (not rejects) with the answer string.
4. **History behavior** — `historySize: 2`, `removeHistoryDuplicates: true`; feed 3 distinct lines and assert only the most recent 2 remain; feed a duplicate of an existing entry and assert the older copy is dropped, newer moved to front; assert `'history'` fires once per mutating line with the current array snapshot.
5. **`rl.write()` programmatic input** — `rl.write('hello')` followed by reading `rl.line`/`rl.cursor`; separately, `rl.write(null, { ctrl: true, name: 'u' })` against a TTY-mode interface clears the line (assert `rl.line === ''` after).
6. **`rl[Symbol.asyncIterator]()`** — `for await (const line of rl)` over 3 fed lines collects them in order; breaking out of the loop early triggers `rl.close()` (assert `'close'` fired and the underlying listener was removed).
7. **`AbortSignal` cancels a pending question** — callback variant: abort before the user answers, assert the callback is never invoked; promises variant: abort mid-wait, assert the promise rejects with the abort reason. Also: an already-aborted signal passed to `question()` rejects/no-ops immediately without waiting for input.
8. **`emitKeypressEvents` + `'keypress'` shape** — against the fake raw-mode TTY stream, feed a raw byte sequence for a printable char and assert `(str, key)` with `key.name`/`ctrl`/`meta`/`shift` populated correctly; feed an arrow-key escape sequence and assert `str === undefined` with the correct `key.name` (`'up'`/`'down'`/`'left'`/`'right'`).
9. **Cursor utility functions** — `readline.cursorTo(stream, 3, 1)`, `readline.moveCursor(stream, -2, 0)`, `readline.clearLine(stream, 0)`, `readline.clearScreenDown(stream)` against the mock `Writable`; assert the exact ANSI escape bytes written match the expected sequences byte-for-byte.
10. **`readlinePromises.Readline` batching** — queue `cursorTo(0,0).clearLine(0).moveCursor(1,1)` without calling `commit()`, assert nothing was written yet; call `commit()`, assert all three sequences flush in order in one (or the expected number of) writes; separately, queue actions then call `rollback()` and assert nothing is ever written. Also test `autoCommit: true` writes immediately per call.
11. **Completer** — sync `(line) => [matches, line]` filtering a fixed word list; async callback-style `(line, cb) => cb(null, [matches, line])`; `Promise`-returning async completer — all three variants exercised against the same Tab-completion trigger path (terminal mode) and asserted to produce identical results.
12. **`'SIGINT'` default behavior** — with no `'SIGINT'` listener registered, simulate Ctrl+C and assert `'pause'` fires (not `'close'`); with a `'SIGINT'` listener registered, assert it fires instead and the interface stays open.
13. **`close()` idempotency / `Symbol.dispose`** — calling `rl.close()` twice does not throw or double-emit `'close'`; `rl[Symbol.dispose]()` produces the identical effect as `rl.close()`; a `using rl = createInterface(...)` block auto-closes at scope exit.
14. **Adjacent-feature combinations** — a `'line'` handler that itself calls `rl.question()` again (nested prompt loop, the canonical REPL pattern); a `try/catch` around a `'line'` handler that throws (assert the interface is not left in a broken listening state); a user `class MyPrompt extends readline.Interface` (verifying `Interface` is a normally-extensible class, not sealed).
15. **Multithread smoke test (per §5.4)** — construct an `Interface` over a mock stream on the main thread; spawn a worker/thread and assert it cannot see/use that `Interface` instance (no accidental sharing); confirm a worker constructing its *own* `Interface` over its own stream works independently and does not interfere with the main thread's raw-mode state.

## 7. Open questions / deferrals

- **`ERR_INVALID_CURSOR_POS` / `ERR_USE_AFTER_CLOSE` exact trigger conditions and messages** — not spelled out in the fetched public API docs; verify against Node's own source (`lib/internal/readline/*.js`, `lib/internal/errors.js`) before finalizing RTS's error parity for `cursorTo`'s edge cases and `question()`-after-`close()`.
- **`Key` object's `sequence`/`code` fields** — the public docs confirm `ctrl`/`meta`/`shift`/`name` (from the `rl.write(data, key)` signature) but do not confirm whether `'keypress'` event listeners additionally receive `sequence`/`code`; marked `(verify)` in §3, to be pinned down against Node's actual `'keypress'` emission source before implementation.
- **`.terminal` as a readable instance property** — the fetched docs describe `terminal` only as a constructor *option*, not a documented instance property; verify whether real Node exposes `rl.terminal` for introspection (believed to exist internally) before deciding whether RTS's `.ts` shim should expose it too.
- **String-width native primitive (§5.1/§5.2/§5.8(l))** — deferred: ship the codepoint-count approximation for P1, add `__RTS_FN_NODE_READLINE_STRING_WIDTH` only if a CJK/emoji-heavy fixture demonstrates visibly incorrect line-wrap/cursor math.
- **Exotic terminal protocols** (kitty keyboard protocol, iTerm2 proprietary escapes, Windows legacy console vs. modern Windows Terminal/ConPTY nuances) — out of scope; RTS targets the same xterm-compatible baseline Node itself targets, not a superset.
- **`readlinePromises.Readline.commit()`'s exact resolution timing** — whether it can resolve on the next microtask after a synchronous/buffered write, or must genuinely wait for a stream write-completion callback, is contingent on `node:stream`'s not-yet-written design (§5.7); revisit once that module's spec exists.
- **Legacy positional `new readline.Interface(input, output, completer, terminal)` constructor form** — noted for completeness in §2 but not planned for RTS implementation unless a real ported program is found to depend on it (the object-form `createInterface()` is the documented, current-idiom entry point).
