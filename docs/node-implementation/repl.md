# node:repl

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:repl` |
| Node.js version | 25.x |
| Stability | 2 - Stable (one member, `repl.builtinModules`, is separately 0 - Deprecated) |
| Tier | P2 |
| Status | [ ] Not implemented — spec only |
| Import forms | `import repl from "node:repl"`; `import { REPLServer, start, Recoverable, REPL_MODE_SLOPPY, REPL_MODE_STRICT, builtinModules } from "node:repl"`; CJS-style `require("node:repl")` / legacy bare `require("repl")` (both resolve to the same module) |
| Globals exposed | None. `node:repl` defines no ambient `globalThis` members; the `_`/`_error` "magic variables" and auto-loaded core-module bindings (e.g. typing `fs` at the prompt) exist only *inside* a running `REPLServer`'s own `context` object, not as real JS globals importers can see. |

## 1. Purpose

`node:repl` implements a Read-Eval-Print-Loop: it reads lines of input from a stream (normally an interactive terminal), evaluates them with a pluggable evaluation function (JavaScript `eval` by default), and writes a formatted result back to an output stream. It is the engine behind the interactive `node` prompt (`node` with no script argument) and is also usable as a library (`repl.start(options)`) to build custom interactive tools, embedded consoles, or debugging shells layered on any `Readable`/`Writable` stream pair — not necessarily a real terminal. `REPLServer` extends `readline.Interface`, so this module is a P2 consumer of `node:readline`'s line-editing/history/terminal machinery layered with REPL-specific semantics (multi-line recovery, persistent context, dot-commands, `_`/`_error`).

## 2. Exported API surface (COMPLETE)

### Classes

#### `class REPLServer extends readline.Interface`

Added in: v0.1.91. Never constructed directly by typical user code — created via `repl.start([options])`; the constructor is documented for completeness and for embedders that want to bypass `start()`'s defaulting.

**Constructor**

```ts
new repl.REPLServer(options: string | ReplOptions)
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `string \| ReplOptions` | no | — (a bare `string` is treated as `{ prompt: options }`) |

Throws: none directly from the constructor; a `terminal: true` option on a non-TTY `input`/`output` pair does not throw (Node falls back gracefully), but a custom `eval` combined with `breakEvalOnSigint: true` throws synchronously (`breakEvalOnSigint` requires the default evaluator — see §4).

**Events** (own; in addition to everything inherited from `readline.Interface`, itself an `EventEmitter`)

| Event | Args | Notes |
|---|---|---|
| `'exit'` | *(none)* | Added v0.7.7. Emitted when the REPL exits via the `.exit` command, Ctrl+C pressed twice on a blank line, or Ctrl+D (EOF) on the input stream. |
| `'reset'` | `(context: object)` | Added v0.11.0. Emitted when the REPL's `context` is reset to a fresh empty object — i.e. after the `.clear` command, but **only** when `useGlobal` is `false` (a global-context REPL's `.clear` re-emits `'reset'` too, but resetting `global` itself is not actually performed — see §4). |

**Events inherited from `readline.Interface`** (documented fully in the `node:readline` spec; listed here only for cross-reference since `REPLServer` emits them too): `'line'`, `'close'`, `'pause'`, `'resume'`, `'history'`, `'SIGINT'`, `'SIGTSTP'` (POSIX only), `'SIGCONT'` (POSIX only).

**Instance properties**

| Property | Type | Notes |
|---|---|---|
| `replServer.context` | `object` | The object whose own enumerable properties appear as local bindings inside the REPL. Writable by default; a property can be made read-only via `Object.defineProperty(replServer.context, name, { configurable: false, enumerable: true, value })`. When `useGlobal` is `true` this **is** `globalThis` itself (not a copy); when `false` it is a REPL-private plain object recreated on every `'reset'`. |

**Instance methods**

| Signature | Returns | Notes |
|---|---|---|
| `replServer.defineCommand(keyword: string, cmd: ReplCommandAction \| ReplCommand): void` | `void` | Added v0.3.0. Registers a new `.<keyword>` dot-command (keyword given *without* the leading `.`). `cmd` is either a plain `action` function, or an object with optional `help` text plus the `action` function. Calling this with a keyword that collides with a built-in command (`break`/`clear`/`exit`/`help`/`save`/`load`/`editor`) overrides the built-in. |
| `replServer.displayPrompt(preserveCursor?: boolean): void` | `void` | Added v0.1.91. Readies the REPL for the next line of input: prints the configured prompt (or `'... '`-style continuation indicator while a multi-line/buffered command is pending) and resumes the input stream. `preserveCursor: true` skips moving the cursor back to column 0. Intended for use from inside a `defineCommand()` action, not general application code. |
| `replServer.clearBufferedCommand(): void` | `void` | Added v9.0.0. Discards any partially-entered multi-line command currently buffered (equivalent to what `.break` does internally). Intended for use from inside a `defineCommand()` action. |
| `replServer.setupHistory(historyConfigOrPath: string \| HistoryConfig, callback?: (err: Error \| null, repl: REPLServer) => void): void` | `void` | Added v11.10.0; `historyConfig`-object form + `onHistoryFileLoaded` added v24.2.0. Initializes persistent history backed by a file. If `historyConfig` is a plain `string` it is the file path (equivalent to `{ filePath: historyConfig }`). `callback` is optional when `historyConfig.onHistoryFileLoaded` is supplied instead; supplying **neither** is a caller error (nothing to be notified when the (async) file I/O completes). |

`REPLServer` additionally inherits every `readline.Interface` instance method unchanged (`question`, `write`, `pause`, `resume`, `close`, `prompt`, `setPrompt`, `getPrompt`, `line`, `cursor`, `getCursorPos`, `[Symbol.asyncIterator]`, …) — see the `node:readline` spec for their full signatures; `REPLServer` overrides `line`-handling internally to add the evaluate/print step but the public method surface is not narrowed.

#### `class Recoverable extends SyntaxError`

Added implicitly with the module (undated in the current docs; present since very early Node versions). Not itself listed as a top-level export separately documented with an "Added in" tag, but is publicly exported as `repl.Recoverable`.

```ts
new repl.Recoverable(err: Error)
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `err` | `Error` | no | — the original error this recoverable wraps (typically a `SyntaxError` from parsing incomplete input) |

Returned (not thrown) to a custom `eval` function's `callback` — `callback(new repl.Recoverable(err))` — to tell the `REPLServer` "this input is incomplete, buffer it and prompt for a continuation line" rather than "this input errored".

### Top-level functions

#### `repl.start(options?: string | ReplOptions): REPLServer`

| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `string \| ReplOptions` | yes | `{}` — a bare string is shorthand for `{ prompt: options }` |

Returns: `REPLServer`. Variant: **sync** (the returned instance then drives its own async read loop over `options.input`). Throws: none of its own; propagates whatever the `REPLServer` constructor throws (see above — `breakEvalOnSigint` + custom `eval` combination).

Creates a `REPLServer`, wires it to `process.stdin`/`process.stdout` by default, and immediately calls `displayPrompt()` to start reading. `handleError` was added in v25.9.0 (see the `ReplOptions` table in §3) as an escape hatch to customize error routing per-instance.

### Properties & constants

| Name | Type | Value / Default | Notes |
|---|---|---|---|
| `repl.REPL_MODE_SLOPPY` | `symbol` | unique symbol | Pass as `options.replMode` to evaluate input in non-strict ("sloppy") mode. This is the default. |
| `repl.REPL_MODE_STRICT` | `symbol` | unique symbol | Pass as `options.replMode` to evaluate input as if under a top-level `'use strict'` — e.g. `with` statements become a `SyntaxError`. |
| `repl.builtinModules` | `string[]` | e.g. `['assert', 'buffer', 'child_process', ...]` | Added v14.5.0. **Deprecated since v24.0.0/v22.16.0** (Stability 0) — use `module.builtinModules` (`node:module`) instead. An automated codemod (`npx codemod@latest @nodejs/repl-builtin-modules`) exists upstream to migrate call sites. |

### Events

Covered under the `REPLServer` class above (`'exit'`, `'reset'`, plus everything inherited from `readline.Interface`). There are no module-level (non-instance) events for `node:repl`.

## 3. Types & option objects

```ts
/** Options accepted by `repl.start()` and `new repl.REPLServer()`. */
interface ReplOptions {
  /** Input prompt to display. Default: `'> '` (note the trailing space). */
  prompt?: string;

  /** Readable stream input is read from. Default: `process.stdin`. */
  input?: NodeJS.ReadableStream;

  /** Writable stream output is written to. Default: `process.stdout`. */
  output?: NodeJS.WritableStream;

  /**
   * Treat `output` as a TTY/terminal (enables ANSI line-editing, colors,
   * previews). Default: `output.isTTY` if present, else `false`.
   */
  terminal?: boolean;

  /**
   * Custom evaluation function. Default: an async wrapper around JavaScript
   * `eval()` against `context`. May signal "incomplete input, need another
   * line" by calling `callback(new repl.Recoverable(err))`.
   */
  eval?: ReplEvalFunction;

  /**
   * Enable ANSI color styling in the default `writer`. Ignored if a custom
   * `writer` is supplied. Default: color-support probe of `terminal`.
   */
  useColors?: boolean;

  /**
   * If `true`, default evaluation uses `globalThis` as `context` (so
   * REPL-declared globals are visible outside the REPL too). If `false`,
   * a private context object is used instead. Default: `false`.
   */
  useGlobal?: boolean;

  /**
   * If `true`, the default `writer` suppresses printing when the evaluated
   * result is `undefined`. Default: `false`.
   */
  ignoreUndefined?: boolean;

  /**
   * Formats the evaluated result before writing to `output`.
   * Default: `util.inspect()` (with `showProxy: true`, `colors: useColors`).
   */
  writer?: ReplWriterFunction;

  /** Custom Tab-completion function. See `node:readline`'s completer shape. */
  completer?: ReplCompleterFunction;

  /**
   * `repl.REPL_MODE_SLOPPY` (default) or `repl.REPL_MODE_STRICT` — whether
   * the default evaluator runs input in sloppy or strict mode.
   */
  replMode?: symbol;

  /**
   * If `true`, a SIGINT (Ctrl+C) received while the default evaluator is
   * mid-evaluation aborts that evaluation instead of exiting/prompting.
   * Mutually exclusive with a custom `eval` (throws at construction if both
   * are set). Default: `false`.
   */
  breakEvalOnSigint?: boolean;

  /**
   * Print autocomplete and evaluated-output previews as the user types.
   * No effect unless `terminal` is truthy. Default: `true` with the default
   * evaluator, `false` with a custom `eval`.
   */
  preview?: boolean;

  /**
   * Added v25.9.0. Customizes how an error surfacing during evaluation is
   * handled. Return `'print'` (default behavior: print to `output`),
   * `'ignore'` (skip all further error handling for this error), or
   * `'unhandled'` (forward to process-wide `uncaughtException`/`unhandledRejection`).
   */
  handleError?: (err: unknown) => 'print' | 'ignore' | 'unhandled';
}

/** Custom evaluation function shape (`options.eval`). */
type ReplEvalFunction = (
  code: string,
  context: object,
  replResourceName: string,
  callback: (err: Error | null | undefined, result?: unknown) => void,
) => void;

/** Custom output-formatting function shape (`options.writer`). */
type ReplWriterFunction = (output: unknown) => string;

/** Custom Tab-completion function shape (`options.completer`); mirrors
 *  `node:readline`'s completer contract. */
type ReplCompleterFunction = (
  line: string,
  callback?: (err: Error | null, result: [completions: string[], originalSubstring: string]) => void,
) => [completions: string[], originalSubstring: string] | void;

/** `historyConfig` accepted by `replServer.setupHistory()`. */
interface HistoryConfig {
  /** Path to the history file. */
  filePath: string;
  /** Maximum retained history lines; `0` disables history. Default: `30`. */
  size?: number;
  /** Drop older duplicate lines when re-adding an identical entry. Default: `false`. */
  removeHistoryDuplicates?: boolean;
  /** Called once history is ready for writes, or on load error. */
  onHistoryFileLoaded?: (err: Error | null, repl: REPLServer) => void;
}

/** Shape accepted by `replServer.defineCommand()`'s second argument. */
type ReplCommandAction = (this: REPLServer, text: string) => void;

interface ReplCommand {
  /** One-line help text shown by the `.help` command. */
  help?: string;
  /** The command's implementation; receives the remainder of the input line
   *  after the command keyword (trimmed), may be empty string. */
  action: ReplCommandAction;
}
```

## 4. Node semantics & edge cases

- **Terminal vs non-terminal auto-detection.** `terminal` defaults from `output.isTTY`; when falsy (e.g. `output` is a pipe, a file, or an in-memory stream used in tests), advanced features are unavailable: no ANSI colors, no reverse-i-search, no Tab-completion preview, no multi-line editor niceties — the REPL degrades to plain line-at-a-time echo, which is exactly the mode most automated tests should drive it in.
- **`_`/`_error` magic variables.** The default evaluator assigns the last evaluated expression's value to `_` and the last thrown/rejected error to `_error`, *unless* the user has explicitly assigned to either (`_ = 5` disables auto-assignment permanently for that session and prints `"Expression assignment to _ now disabled."`). Both are ordinary context properties, not real language magic.
- **`await` at top level.** Enabled by default in the default evaluator (no wrapping `async function` needed by the user). A known limitation: each accepted line is evaluated as its own top-level script, so re-declaring a `const`/`let` name across two different `await`-containing lines can throw `SyntaxError: Identifier 'x' has already been declared` even though it "looks like" separate statements — an artifact of how V8 implements REPL top-level await via per-line wrapping. `--no-experimental-repl-await` disables top-level await entirely (falls back to needing an explicit `async () => { ... }` IIFE).
- **Global uncaught exceptions & `domain`.** The default `REPLServer` wraps evaluation in Node's (deprecated) `domain` module so an uncaught exception thrown *inside evaluated code* doesn't crash the whole process — it's caught, printed, and the REPL keeps running. Two side effects of this domain usage: (1) code entered at the prompt that calls `process.on('uncaughtException', ...)` throws `TypeError [ERR_INVALID_REPL_INPUT]` — you cannot install your own top-level uncaught-exception handler from inside a domain-wrapped REPL session; (2) calling `process.setUncaughtExceptionCaptureCallback()` from inside the REPL throws `ERR_DOMAIN_CANNOT_SET_UNCAUGHT_EXCEPTION_CAPTURE`. Since Node v12.3.0, an uncaught exception in a **standalone** REPL (i.e. `node` with no script) also emits the process-wide `'uncaughtException'` event (in addition to being caught by the domain) — a REPL embedded inside a larger program does not re-emit it globally.
- **Recoverable-error detection is evaluator-specific.** The default evaluator's "is this just unterminated input, not a real syntax error" heuristic pattern-matches the underlying JS engine's `SyntaxError.message` text (e.g. `/^(Unexpected end of input|Unexpected token)/`). This is inherently tied to the specific parser/engine in use — a different parser produces different error message text for the same truncated input, so this heuristic cannot be ported verbatim (see §5.8/§7).
- **History file location & env overrides.** `NODE_REPL_HISTORY` (path; empty string `''` disables persistence entirely; on Windows, one-or-more spaces also disables it) — default `.node_repl_history` in the OS home directory. `NODE_REPL_HISTORY_SIZE` (default `1000`, must be a positive number) caps retained lines. `NODE_REPL_MODE` (`'sloppy'`|`'strict'`, default `'sloppy'`) sets the default `replMode` for the standalone REPL specifically (not for library use of `repl.start()`, which always defaults to `REPL_MODE_SLOPPY` unless the caller passes `replMode` explicitly).
- **`NODE_NO_READLINE=1`** starts the standalone REPL in "canonical" (non-raw) terminal mode, sacrificing line editing/completion for compatibility with external line editors like `rlwrap`.
- **Reverse-i-search (Ctrl+R/Ctrl+S).** ZSH-style bidirectional history search; duplicate history entries are skipped during search; any key not part of the search sequence accepts the current match; Esc or Ctrl+C cancels back to the pre-search input.
- **`.editor` mode.** Enters a raw multi-line paste/edit mode (Ctrl+D submits the whole buffer as one evaluation, Ctrl+C cancels) — bypasses the normal per-line Recoverable-error continuation logic entirely, since the whole block is submitted at once.
- **Auto-`require` of core modules.** The default evaluator lazily does `global.fs = require('node:fs')`-equivalent the first time an undeclared identifier matching a core module name (e.g. `fs`, `http`) is referenced — a convenience with no real "module resolution" semantics; it only fires for **built-in** module names, never arbitrary npm packages.
- **No platform-specific errno/encoding surface.** This module has no filesystem-content or network-byte-encoding concerns of its own beyond `setupHistory`'s plain-text history file (UTF-8 line-per-entry); the meaningful Windows-vs-POSIX difference is entirely about environment-variable disabling conventions (see above) and terminal raw-mode APIs (owned by `node:readline`, not repeated here).
- **Deprecation.** Only `repl.builtinModules` is deprecated (Stability 0); the rest of the module is Stability 2 - Stable.
- **Security notes.** A REPL evaluates arbitrary code with the full privileges of the host process — by design, since it's a developer tool, not something to expose to untrusted input (e.g. never wire a REPL's `input`/`eval` to a network-facing socket without an authorization layer of your own; Node's docs carry no built-in sandboxing guarantee here, and neither should RTS's).

## 5. RTS implementation notes

### 5.1 Native impl mapping

`node:repl` sits almost entirely as a `.ts` shim on top of two things this module does *not* own: `node:readline`'s terminal/line-editing engine (P1/P2 prerequisite, not yet documented — see §5.7/§7) and RTS's own **runtime compile-and-execute** primitive, which already exists in today's runtime layer (`runtime` namespace: `eval_file`/`eval`, backing `rts eval`/dynamic `import()`, currently living under `rts-std`/`rts-runtime`'s `runtime` namespace per the project's own architecture rules). Concretely:

- **Line I/O, raw-mode terminal, Tab-completion, reverse-i-search, prompt display, history keystroke handling** — entirely delegated to `node:readline`'s `Interface` base class. `REPLServer` in RTS is a `.ts` subclass (`class REPLServer extends readline.Interface { ... }`) that overrides the `'line'` handling to run the evaluate/print step instead of readline's default "just emit `'line'`" behavior. **No new terminal-handling Rust code belongs in `node:repl` itself** — it is entirely `node:readline`'s job (crossterm-or-equivalent raw mode, ANSI codes, cursor movement).
- **Default evaluation (`eval` option default)** — unlike real Node (which calls V8's native `eval()` synchronously in-process), RTS has no embedded JS/TS interpreter to fall back on: RTS *compiles* TypeScript/JavaScript ahead of time (or JIT, per line, in this case) via the same Cranelift pipeline used for `rts run`. The default evaluator therefore must: (1) parse the submitted line with the same SWC-based parser RTS already uses; (2) on a parse error at end-of-input, decide "recoverable" (needs continuation) vs "real error" using RTS's *own* parser diagnostics (not V8's message-text heuristic — see §5.8/§7); (3) wrap a bare trailing expression statement in a synthetic `return (...)` (or reuse the same "is this an ExpressionStatement, if so capture its value" trick V8's own REPL evaluator uses) so the result can be captured and handed to the `writer`; (4) JIT-compile the wrapped snippet against a **persistent per-session global scope** (so a `let`/`const`/`function` declared on one line is visible on the next) and invoke it, reusing the existing `runtime.eval`-style compile-at-runtime primitive rather than reimplementing a parser/codegen entry point.
- **Persistent cross-line context (`replServer.context`)** — the central RTS-specific design problem this module introduces (not present in any prior `rts-node` module): each accepted line is its own independent JIT compilation unit, yet Node-parity requires `let x = 1` on line 1 to be visible as `x` on line 2. RTS's engine does not otherwise recompile-and-extend a running program incrementally. The natural mapping is a **growing table of named module-level gcells** — analogous to the "mutable global env-record capture" work already partially landed for closures (#195) — keyed by the identifier the REPL line declared, resolved by name at the start of each subsequent line's compile. `context` (the JS-visible object) is then a thin `.ts` Proxy-or-plain-object view over that gcell table (get/set trap → gcell read/write), not a literal object the runtime hands back.
- **History persistence (`setupHistory`)** — plain UTF-8 line-oriented file I/O; backed directly by `std::fs`/`std::io` inside `rts-node` (its own dependency, independent of `rts-std`), not shared with any other module's file-handling.
- **`util.inspect()`-based default `writer`** — depends on `node:util`'s `inspect()` (a sibling `rts-node` module). Until that lands, ship a minimal internal formatter (typeof-based dispatch: primitives via `String()`, arrays/objects via a bounded-depth JSON-ish printer) and swap in real `util.inspect()` once available (this is an intra-`rts-node` dependency, not a doctrine violation).
- **`Recoverable`** — a plain `.ts` class extending the ambient primordial `Error`/`SyntaxError` (RTS's own `Error` subclass mechanism, already primordial); no native code needed.
- **`domain`-based uncaught-exception routing** — RTS has no `node:domain` implementation plan beyond source-compatibility stubs (see the sibling `domain.md` spec, itself Stability 0/deprecated). `node:repl`'s default evaluator instead wraps each line's execution in a plain `try/catch` (RTS's existing try/catch primitive) and reports the caught error through `handleError`/the default `'print'` path — this is a **documented, deliberate divergence** from Node's `domain`-based mechanism (see §7); the observable behavior (uncaught error inside evaluated code doesn't kill the REPL) is preserved even though the mechanism differs.

### 5.2 ABI surface

Following the `__RTS_FN_NODE_<MODULE>_<NAME>` convention (module = `REPL`), plus reuse of the existing runtime compile-and-run primitive.

| Symbol | Args (`AbiType`) | Returns | Purpose |
|---|---|---|---|
| `__RTS_FN_NODE_REPL_COMPILE_AND_RUN` | `(session: Handle, source: StrPtr, filename: StrPtr)` | `Handle` (a tagged result: ok-value / recoverable-error / hard-error, unwrapped `.ts`-side) | Compiles `source` against the persistent gcell table owned by `session` and executes it; the actual heavy lifting (parse/HIR/Cranelift JIT) is the **same underlying primitive** `rts run`/dynamic-`import()` already uses — this symbol is a thin session-scoped wrapper, not a new compiler entry point. |
| `__RTS_FN_NODE_REPL_SESSION_NEW` | `()` | `Handle` | Allocates a new persistent-gcell-table session (one per `REPLServer`, or one per `useGlobal: true` REPL sharing the program's real global table instead — see below). |
| `__RTS_FN_NODE_REPL_SESSION_FREE` | `(session: Handle)` | `Void` | Releases the session's gcell table. |
| `__RTS_FN_NODE_REPL_SESSION_RESET` | `(session: Handle)` | `Void` | Backs `.clear` / the `'reset'` event: drops all accumulated declarations, starting the next compile from an empty table. |
| `__RTS_FN_NODE_REPL_IS_RECOVERABLE` | `(diagnostic: StrPtr)` | `Bool` | Given RTS's own parser diagnostic text/kind for a failed parse, decides "needs a continuation line" vs "real syntax error" — RTS-specific replacement for V8's message-text regex (see §5.1/§7). |
| `__RTS_FN_NODE_REPL_HISTORY_LOAD` | `(path: StrPtr, max_lines: I64)` | `Handle` (opaque history-buffer handle, or 0/null-handle if the file doesn't exist yet) | Reads up to `max_lines` trailing lines from the history file at startup. |
| `__RTS_FN_NODE_REPL_HISTORY_APPEND` | `(history: Handle, line: StrPtr, dedup: Bool)` | `Void` | Appends one accepted line; `dedup` implements `removeHistoryDuplicates`. |
| `__RTS_FN_NODE_REPL_HISTORY_FLUSH` | `(history: Handle)` | `Void` | Forces pending history writes to disk (called on `'exit'`/`close`). |

Rich objects as opaque `Handle`s: the REPL **session** (persistent gcell table + associated compile state) and the **history buffer**. `REPLServer` itself, its `context` view, dot-command dispatch (`.break`/`.clear`/`.exit`/`.help`/`.save`/`.load`/`.editor`/`defineCommand`), the `_`/`_error` bookkeeping, and all event wiring (`'exit'`, `'reset'`, plus inherited `readline.Interface` events) are a `.ts` shim (`crates/rts-node/src/repl/repl.ts`) over these externs plus `node:readline`'s own extern surface (out of this module's scope). `Recoverable`, `REPL_MODE_SLOPPY`/`REPL_MODE_STRICT` (plain `.ts`-level `Symbol()` values, no native backing needed), and `repl.builtinModules` (delegates to `node:module`'s `builtinModules` list) are pure `.ts`.

### 5.3 Async model

- **Reading input lines** is inherently async/event-driven (`node:readline`'s `'line'` event fires whenever the terminal driver has a complete line) — `node:repl` itself adds no new async primitive here; it reacts to `readline.Interface`'s existing event-loop-driven input handling.
- **The default evaluator's compile+run step** is, per today's interim async model (`docs/specs/async-promise-function.md`: "the new engine's interim async is SYNCHRONOUS"), executed **synchronously on the calling thread** relative to the REPL's own event loop — i.e. one accepted line blocks the REPL from accepting the next until evaluation finishes, matching real Node's own single-threaded synchronous-eval-per-line behavior closely enough for parity purposes. A line that itself contains `await somePromise` bridges into the existing Promise subsystem's `promise.wait()` (blocking wait on a oneshot), consistent with how top-level `await` already works elsewhere in RTS.
- **A custom `eval` function** (Node's `(code, context, replResourceName, callback) => void` shape) is inherently callback-style/async-capable — RTS's `.ts` shim must support a user's custom evaluator that itself does async work (e.g. calls out to a network service) before invoking `callback`; this composes with the existing Promise/callback bridging patterns already used elsewhere in `rts-node` (Node-style `(err, result) => void` callback conventions).
- **History file I/O** (`setupHistory`, `HISTORY_LOAD`/`APPEND`/`FLUSH`) is small, local-disk, line-oriented text I/O — implemented as ordinary blocking `std::fs` calls inside `rts-node`'s own Rust code; does not need the shared tokio runtime (no meaningful concurrency benefit for a REPL's own history file, which is accessed from exactly one thread at a time in the common case).
- **`breakEvalOnSigint`** requires the ability to *interrupt* an in-flight synchronous evaluation from a signal handler — this needs the same infrastructure `node:process`'s `SIGINT` handling and `node:readline`'s Ctrl+C detection already require; flagged as a cross-module dependency, not unique async infra.

### 5.4 Multithread / worker interaction

- A `REPLServer` instance (its `context`/gcell-table session, its buffered partial-input state, its history buffer) is **ordinary per-thread-region data** under `docs/specs/rts-threading-model.md` — nothing here needs `shared`/promotion-on-publication. Real Node never shares one `REPLServer` object across `worker_threads` either (a REPL is fundamentally tied to one input/output stream pair and one evaluation thread); RTS should preserve exactly this restriction.
- If a REPL is started **inside** a `worker_threads` worker (an unusual but not-forbidden pattern — e.g. a debug console per worker), its gcell-table session must live in that worker's own thread-local region, entirely separate from the main thread's globals or any other worker's — i.e. `useGlobal: true` inside a worker binds to *that worker's* global object, never the main thread's.
- The history file on disk is the one place true cross-instance (even cross-process) contention could occur if two `REPLServer`s somehow pointed `setupHistory` at the same path concurrently; follow the existing namespace-state pattern (`Arc<Mutex<HistoryState>>` per open file path, `OnceLock`-initialized) documented in `.claude/rules/02-runtime.md` rather than assuming single-writer.

### 5.5 Buffer / TypedArray interop

Minimal. The module's own I/O is exclusively `StrPtr` text (prompt strings, input lines, output-writer strings, `.save`/`.load` file paths and their UTF-8 file contents, history file lines). If a value a user evaluates at the prompt happens to be a `Buffer`/`TypedArray`, it flows through the evaluation result as an ordinary JS value and is formatted by the `writer` (ultimately `util.inspect()`, which already knows how to pretty-print typed arrays) — no dedicated Buffer ABI is needed in `node:repl` itself.

### 5.6 Doctrine placement

- `node:repl` is entirely **non-primordial**: `REPLServer`, `Recoverable`, `repl.start`, `REPL_MODE_SLOPPY`/`STRICT`, `builtinModules` have no native literal/syntactic form — the engine must never hardcode any of these names.
- **Resolution path:** `import { REPLServer } from "node:repl"` → `"node:"`-prefixed specifier → `rts-node`'s `NODE_SPECS` data table via `ns_prefix_for("node:repl")` → `Some("node_repl")` → mounts `crates/rts-node/src/repl/repl.ts` under the `"node:repl"` specifier (same mechanism as every other `rts-node` module), plus registers the small `NodespaceSpec::members` table in §5.2 so `node_lookup("node_repl.compile_and_run")` etc. resolve to their externs. Exactly the "data table, never a hardcoded arm in codegen" pattern the doctrine requires.
- **Native-extern vs `.ts`-shim split:** the compile-and-run bridge, session/gcell-table lifecycle, recoverable-error classification, and history file I/O are native externs (§5.2); everything JS-shaped (the `REPLServer` class itself, dot-commands, `context` proxy view, `_`/`_error` bookkeeping, event wiring, `Recoverable`/mode-symbol/`builtinModules` values) is a `.ts` shim (`crates/rts-node/src/repl/repl.ts`), consistent with every other `rts-node` module's split.
- `REPLServer extends readline.Interface` is a **cross-module** `.ts`-level `extends` — `node:repl`'s shim imports `node:readline`'s exported `Interface` class the same way ordinary user TypeScript would, not via any special engine hook.

### 5.7 Shared-infra dependencies (FLAG)

- **`node:readline`'s `Interface` class and its native terminal/line-editing externs.** `REPLServer` cannot exist without it — raw-mode input, prompt/cursor rendering, Tab-completion, and history-keystroke handling (Ctrl+R/Ctrl+S, arrow-key history recall) are entirely `node:readline`'s job. **`node:readline` is not yet documented/implemented in `rts-node`** (no `docs/node-implementation/readline.md` exists as of this writing) — it is a **hard prerequisite**, not an optional nice-to-have; `node:repl` cannot be built before it.
- **RTS's runtime compile-and-execute primitive** (today: the `runtime` namespace's `eval`/`eval_file`, i.e. `runtime_eval_src_jit`, living under `rts-std`/`rts-runtime`). Since `rts-node` cannot depend on `rts-std`, the underlying "parse this source string + JIT-compile it + run it against a supplied/extendable global scope" capability must be reachable from a crate `rts-node` can depend on (the compiler pipeline itself — SWC parse, HIR lowering, Cranelift JIT — already lives in `rts-codegen-new`/`rts-hir`/`rts-adapters`/`rts-engine`, which are *below* the `rts-std` cut line; the specific extern-callable **wrapper** around it is what currently sits in `rts-std`/`rts-runtime` and needs either duplicating minimally inside `rts-node` or hoisting the thin wrapper itself to a shared low crate). This is the single biggest shared-infra risk in this spec — flag prominently for the implementer.
- **Persistent/incrementally-extended global scope across separate JIT compiles.** Not exactly "infra that lives in rts-std" so much as a genuinely new engine capability this module is the first to need: today's JIT/AOT compiles a whole program once; nothing in the existing pipeline "extends" a previously-compiled program's global scope with a second, later, independently-compiled snippet. This may require new support in `rts-codegen-new`/`rts-adapters` (a growable named-gcell table keyed by identifier, resolved per-compile) rather than being purely an `rts-node`-local concern. Flag for design review before implementation starts.
- **The Promise/`await` subsystem** (`promise.wait`, per `docs/specs/async-promise-function.md`) for evaluating a line containing top-level `await` — currently under `rts-std`. Same hoisting need as noted in the `node:events` spec's §5.7 (microtask/await plumbing needs to be reachable without an `rts-std` dependency) — this module inherits that same flag rather than introducing a new one.
- **`node:util`'s `inspect()`** for the default `writer` — an intra-`rts-node` (sibling-module) dependency, not a doctrine violation, but sequencing matters: ship the minimal internal formatter first (§5.1) and swap in real `util.inspect()` once that module exists.
- **`node:process`'s `SIGINT` handling** for `breakEvalOnSigint` and the Ctrl+C-twice-to-exit behavior — likewise an intra-`rts-node` sibling dependency (`node:process`), not `rts-std`.
- **No dependency** on `net`, `tls`, `crypto`, `fs`-beyond-plain-history-files, or the shared tokio runtime for anything in this module — noted explicitly so a later implementer doesn't go looking for one.

### 5.8 Implementation phases

(a) **Prerequisite check:** confirm `node:readline`'s `Interface` class (line reading, `'line'` event, `question`/`write`/`setPrompt`, raw-mode detection) is implemented and stable enough to extend; if not, this is where focus shifts to unblock (`node:readline` spec first — see §5.7).

(b) Scaffold `crates/rts-node/src/repl/mod.rs` with the `NodespaceSpec`/`ns_prefix: "node_repl"` entry (initially with the session/history externs from §5.2 only) registered in `NODE_SPECS`, plus `crates/rts-node/src/repl/repl.ts` exporting a bare `class REPLServer extends readline.Interface` skeleton, `repl.start()`, and the `ReplOptions` type — enough for `import repl from "node:repl"; repl.start()` to read a line and echo it back via the default (not-yet-real) evaluator.

(c) Implement `__RTS_FN_NODE_REPL_SESSION_NEW/FREE/RESET` + the minimal persistent-gcell-table extension to the compiler (flagged in §5.7 as needing design review) — this is the load-bearing new engine capability; land it as its own focused sub-effort before wiring the rest of the evaluator around it.

(d) Implement `__RTS_FN_NODE_REPL_COMPILE_AND_RUN`: parse the line with RTS's existing SWC-based parser, detect bare-expression-statement (wrap to capture value), JIT-compile against the session's gcell table, execute, return ok-value/error.

(e) Implement `__RTS_FN_NODE_REPL_IS_RECOVERABLE` tuned to RTS's/SWC's actual parse-error shapes (not V8's message-text regex) + wire `repl.Recoverable` + the multi-line continuation-prompt behavior in the `.ts` shim.

(f) Wire `_`/`_error` assignment-and-disable-on-explicit-set bookkeeping, `ignoreUndefined`, the minimal internal `writer` fallback (§5.1), and `useColors` (delegates to whatever ANSI-capability probe `node:readline`/`node:tty` exposes).

(g) Implement the built-in dot-commands (`.break`, `.clear`, `.exit`, `.help`, `.save`, `.load`, `.editor`) + `defineCommand()`/`displayPrompt()`/`clearBufferedCommand()`.

(h) Implement `setupHistory()` + `HISTORY_LOAD`/`APPEND`/`FLUSH` externs + `NODE_REPL_HISTORY`/`NODE_REPL_HISTORY_SIZE` env-var defaults; reverse-i-search itself stays `node:readline`'s responsibility (this module only needs to *feed* it persisted lines).

(i) Wire top-level `await` inside evaluated lines to the existing Promise `promise.wait()` bridge (§5.3), and `breakEvalOnSigint` to whatever SIGINT-interrupt mechanism `node:process`/`node:readline` exposes.

(j) `useGlobal: true` support (bind `context` directly to the program's real global object rather than a private session table) + the `'reset'`/`.clear` semantics distinguishing global vs private context.

(k) Swap the minimal internal `writer` for real `util.inspect()` once `node:util` lands; add `repl.builtinModules` as a thin delegate to `node:module`'s `builtinModules` (both ship the same deprecation note upstream carries).

(l) Cross-runtime fixtures (§6); wire into the existing test harness, driving `input`/`output` with in-memory streams (no real TTY needed for the vast majority of behavior).

## 6. Test plan

`tests/node-repl/*.test.ts` (`rts:test` `describe`/`test`/`expect` template). All fixtures drive a `REPLServer` with **in-memory `input`/`output` streams** (a scripted `Readable` feeding pre-written lines + a `Writable` capturing output into a string buffer) rather than a real terminal, since `terminal: false` is exactly the deterministic, script-friendly mode this module must also support well.

1. **Basic eval/print** — `repl.start({ input, output, terminal: false })`; write `1 + 1\n`; assert output contains `2`. Write `const m = 2\n` then `m + 1\n`; assert `3` — regression test for cross-line persistent context (§5.1's core new capability).
2. **`_`/`_error` behavior** — evaluate `[1,2,3]` then `_.length` → `3`; explicitly `_ = 10` then evaluate `1+1` → assert `_` still reads `10` afterward (auto-assignment disabled) and that the disable message is printed once.
3. **Recoverable multi-line input** — write an unterminated `function f(a) {` on one line, then `  return a + 1;` then `}` then `f(5)`; assert the REPL buffered the partial input across the continuation prompts and evaluated correctly at the end (or produces a real `SyntaxError` for genuinely invalid input, e.g. `)))`, without hanging in continuation mode forever).
4. **`ignoreUndefined`** — a `console.log`-only statement (which itself prints, then evaluates to `undefined`) with `ignoreUndefined: true` produces no extra `undefined` line; with it `false` (default) it does.
5. **Custom `eval`** — construct with a custom `(code, context, name, cb) => cb(null, code.toUpperCase())`-shaped evaluator; assert the custom function's output round-trips through the (also possibly custom) `writer` unmodified by the default engine-compile path.
6. **Custom `writer`** — pass a `writer` that uppercases the stringified result; assert output reflects it, decoupled from `useColors`.
7. **`useColors`/ANSI stripping** — with `terminal: true, useColors: true` on a captured in-memory output, assert ANSI escape sequences are present around the printed value; with `useColors: false`, assert none.
8. **`defineCommand` custom dot-command** — register `.double` with an `action(text)` that evaluates `Number(text) * 2` and writes it; drive `.double 21\n`; assert `42` printed; assert `.help` output lists the custom command's `help` text.
9. **`displayPrompt`/`clearBufferedCommand`** — inside a custom command's `action`, call `clearBufferedCommand()` mid-multi-line-buffer and assert the next line is evaluated fresh (not appended to the discarded buffer); assert `displayPrompt(true)` doesn't reset cursor column (best-effort assertion against the captured output bytes).
10. **`'exit'` event** — write `.exit\n`; assert the `'exit'` event fires exactly once with no arguments; separately, closing the input stream (simulated EOF) also fires `'exit'`.
11. **`'reset'` event + `.clear`** — with `useGlobal: false`, declare `let x = 1`, then `.clear`, then reference `x`; assert a `ReferenceError`-shaped evaluation error (x no longer defined) and that `'reset'` fired with a fresh context object (`!==` the previous one).
12. **`useGlobal: true`** — declare `var g = 5` inside the REPL; assert it is visible as `globalThis.g` (or the RTS-equivalent global binding) from outside the REPL session in the same test process.
13. **`setupHistory`** — point at a temp file path; feed 3 lines; close the REPL (flush); start a **new** `REPLServer` pointed at the same file; assert the loaded history contains those 3 lines (order-preserved), honoring `size` truncation and `removeHistoryDuplicates` when a 4th line duplicates the 1st.
14. **`replMode: REPL_MODE_STRICT`** — a construct only valid in sloppy mode (e.g. an undeclared bare assignment `y = 1` without `var`/`let`/`const`, if RTS's strict-mode semantics reject it the same way V8's do) throws under `REPL_MODE_STRICT` but succeeds under the default `REPL_MODE_SLOPPY`.
15. **Top-level `await`** — evaluate `await Promise.resolve(123)\n`; assert `123` printed; evaluate `await Promise.reject(new Error('x'))\n`; assert an "Uncaught Error: x" print and that `_error.message === 'x'` on the following line.
16. **Custom `completer`** — provide a `completer` that always returns a fixed candidate list for a given prefix; drive a Tab-completion request (via whatever `node:readline`-level API triggers it) and assert the candidate list surfaces unmodified.
17. **`handleError` (v25.9.0 option)** — supply `handleError: () => 'ignore'`; evaluate code that throws; assert nothing is printed to `output` for that error (vs. the default `'print'` behavior in test 15).
18. **Non-terminal (`terminal: false`) plain-pipe mode** — reconfirm every above test's core assertions hold with no ANSI/preview/completion machinery involved, since this is the mode RTS's own test harness effectively always uses.
19. **Multithread smoke test (per §5.4)** — start a `REPLServer` inside a `worker_threads` worker (once `node:worker_threads` exists) with its own in-memory streams; assert its `context`/declared bindings are invisible from the main thread and vice versa; deferred until `worker_threads` lands (mark skipped/pending in the test file with a tracking comment, per §7).

## 7. Open questions / deferrals

- **`node:readline` is an unimplemented hard prerequisite.** This spec documents `node:repl`'s own surface fully, but implementation cannot start until `node:readline`'s `Interface` (line reading, raw-mode terminal handling, history keystrokes, Tab-completion plumbing) exists in `rts-node`. Recommend writing that module's spec/implementation immediately before or alongside this one.
- **Persistent cross-line global scope is new engine territory.** Nothing in RTS's existing single-shot JIT/AOT compile model "extends" an already-compiled program's global scope with a later, independently-parsed snippet. The growing-named-gcell-table design sketched in §5.1/§5.2/§5.7 needs a design spike/review with the engine owners before implementation — it may have soundness implications for the `Repr` lattice (a REPL-declared variable's representation can't be proven monomorphic across compiles the way a normal single-compilation-unit local can, so it likely must default to `Tagged` always inside REPL sessions; this is an acceptable, REPL-only performance cost, not a correctness concern, but should be stated explicitly rather than discovered mid-implementation).
- **Recoverable-error heuristic must be re-derived for RTS's own parser (SWC), not ported from V8's message-text regex.** Needs its own small research pass over what SWC's actual EOF-vs-real-syntax-error diagnostics look like for common truncated inputs (unterminated `{`, unterminated string/template literal, dangling binary operator, etc.) before `IS_RECOVERABLE` can be implemented correctly; flagged rather than guessed at in this spec.
- **`domain`-based uncaught-exception semantics are deliberately NOT ported 1:1.** RTS substitutes a plain `try/catch` around each line's evaluation (§5.1) since `node:domain` itself is a separate, deprecated, low-priority module (see the sibling `domain.md` spec) not worth building out just to back this one feature. `ERR_INVALID_REPL_INPUT`/`ERR_DOMAIN_CANNOT_SET_UNCAUGHT_EXCEPTION_CAPTURE`-parity edge cases (§4) are therefore explicitly **out of scope** unless a future consumer needs them — document this divergence in the eventual PR, per the project's "regress when necessary, explicitly" rule.
- **Auto-`require` of core modules at the prompt** (typing `fs` auto-binds `global.fs = require('node:fs')`) depends on RTS's own module-resolution/import mechanism recognizing bare core-module identifiers dynamically inside an already-running compiled snippet — a convenience feature, not core to REPL correctness; recommend deferring to a later phase (not in §5.8's list above) once the core eval/context/history loop is solid.
- **`preview` (autocomplete + evaluated-output previews as you type)** depends on `node:readline` exposing a per-keystroke evaluation hook fast enough to not visibly lag; treat as a stretch feature layered on top of a working non-preview REPL, not a blocking requirement for an initial landing.
- **`util.inspect`'s `showProxy`/`replDefaults`/`compact` mutation-from-inside-the-REPL behavior** (`util.inspect.replDefaults.compact = false`) depends on `node:util` maturity; deferred alongside the general `util.inspect()` dependency noted in §5.7.
- **Performance** is a non-goal for this module by nature (interactive human-paced input); no native fast-path/intrinsic work is proposed anywhere in this spec.
