# node:path

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:path` (+ `node:path/posix`, `node:path/win32`) |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P0 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import path from 'node:path'`; `import { posix, win32 } from 'node:path'`; `import posix from 'node:path/posix'`; `import win32 from 'node:path/win32'`; `const path = require('node:path')`; `const { posix, win32 } = require('node:path')`; `const posix = require('node:path/posix')`; `const win32 = require('node:path/win32')` |
| Globals exposed | none (no ambient global; every access is via an explicit `node:path` / `node:path/posix` / `node:path/win32` import) |

## 1. Purpose

`node:path` provides pure, synchronous, allocation-only utilities for working
with file and directory path **strings** — it does no filesystem I/O (it never
checks whether a path exists) and does no lexical validation beyond string
shape. It normalizes, joins, splits, and re-composes paths (`join`, `resolve`,
`normalize`, `relative`, `parse`, `format`), classifies them (`isAbsolute`,
`matchesGlob`), and extracts components (`basename`, `dirname`, `extname`). The
module's most important property is that its behavior is **OS-flavor
dependent**: `path` (the default export) implements POSIX or Windows semantics
depending on the platform Node itself was built for, while `path.posix` and
`path.win32` (and the `node:path/posix` / `node:path/win32` sub-import
specifiers, since Node v15.3.0) give access to *both* flavors on *any* host so
code can deliberately manipulate paths for the other platform (e.g. a Linux
build tool computing Windows-style paths for a `.bat` script it emits).

## 2. Exported API surface (COMPLETE)

### Classes

None. `node:path` exports no classes/constructors — every export is a
function, a string constant, or the two nested platform namespace objects
(`posix`, `win32`), which are themselves plain objects implementing the same
function surface, not class instances.

### Top-level functions

All functions below are pure, **synchronous**, and side-effect-free (no I/O,
no OS calls) with the sole exception of `resolve()`, which reads the process's
current working directory (and, on Windows, potentially a per-drive working
directory) when no absolute path segment is supplied. Every function is
available identically as `path.<fn>`, `path.posix.<fn>`, and `path.win32.<fn>`
unless noted otherwise.

#### `path.basename(path[, suffix])`

| Param | Type | Optional | Default |
|---|---|---|---|
| `path` | `string` | no | — |
| `suffix` | `string` | yes | — (no suffix stripped) |

Returns: `string` — the last portion of `path` (after the final separator),
with `suffix` removed from the end if present and if `path` isn't identical to
`suffix` after trimming. Throws: `TypeError` (`ERR_INVALID_ARG_TYPE`) if `path`
is not a string, or if `suffix` is given and is not a string. Variant: sync.

Notes: trailing separators in `path` are ignored before extracting the
basename. Suffix comparison is **always byte/case-sensitive**, even under
`path.win32` — `path.win32.basename('C:\\foo.HTML', '.html')` returns
`'foo.HTML'` (unchanged) because `'.HTML' !== '.html'`, despite Windows
filesystems normally being case-insensitive for lookups.

#### `path.dirname(path)`

| Param | Type | Optional |
|---|---|---|
| `path` | `string` | no |

Returns: `string` — the directory name of `path` (everything before the last
separator, ignoring trailing separators). Throws: `TypeError` if `path` is not
a string. Variant: sync.

#### `path.extname(path)`

| Param | Type | Optional |
|---|---|---|
| `path` | `string` | no |

Returns: `string` — from the last `.` in the last portion of `path` to the end
of the string, or `''` if there is no `.` in the last portion, or if the only
`.`s are **leading** dots of an otherwise-dot-only/hidden-file name. Throws:
`TypeError` if `path` is not a string. Variant: sync.

Examples: `extname('index.html')` → `'.html'`; `extname('index.coffee.md')` →
`'.md'`; `extname('index.')` → `'.'`; `extname('index')` → `''`;
`extname('.index')` → `''`; `extname('.index.md')` → `'.md'`.

#### `path.format(pathObject)`

| Param | Type | Optional |
|---|---|---|
| `pathObject` | `FormatInputPathObject` (see §3) | no |

Returns: `string` — the composed path. Throws: `TypeError`
(`ERR_INVALID_ARG_TYPE`) if `pathObject` is not an object. Variant: sync.

Composition rules (evaluated in this priority order):
1. If `pathObject.dir` is set, it is used as the leading directory segment and
   `pathObject.root` is **ignored**; otherwise `pathObject.root` is used
   (`root` is used **as-is**, unlike `dir`, which gets a separator appended).
2. If `pathObject.base` is set, it is appended as-is and `pathObject.ext`
   **and** `pathObject.name` are **ignored**.
3. Otherwise, `pathObject.name` + `pathObject.ext` are concatenated to form the
   base. Since Node 19.0.0, a leading `.` is **automatically inserted** before
   `ext` if `ext` is non-empty and does not already start with `.` (so
   `{ name: 'file', ext: 'txt' }` → `'file.txt'`, not `'filetxt'`).

#### `path.isAbsolute(path)`

| Param | Type | Optional |
|---|---|---|
| `path` | `string` | no |

Returns: `boolean`. Throws: `TypeError` if `path` is not a string. Variant:
sync.

POSIX: a path is absolute iff it starts with `/`. Windows: a path is absolute
if it has a drive letter and backslash root (`C:\...`), a bare UNC/root-relative
`\\...` or `//...`, but **not** a drive-relative path like `C:foo` (that one is
`false` — it depends on that drive's current working directory) nor a
rootless path like `\foo` alone in some edge framings — Node's real behavior
is: `path.win32.isAbsolute('//server')` → `true`,
`path.win32.isAbsolute('\\\\server')` → `true`, `path.win32.isAbsolute('C:/foo/..')`
→ `true`, `path.win32.isAbsolute('bar\\baz')` → `false`,
`path.win32.isAbsolute('.')` → `false`. A zero-length string is never absolute.
Node explicitly documents this function is **not sufficient to defend against
path traversal** (see §4 security notes).

#### `path.join(...paths)`

| Param | Type | Optional |
|---|---|---|
| `...paths` | `string` (variadic, 0..N segments) | yes |

Returns: `string` — all segments joined using the platform separator, then run
through the equivalent of `normalize()`. Throws: `TypeError` if any segment is
not a string. Variant: sync.

Zero-length segments are skipped. If the joined+normalized result is a
zero-length string, `'.'` is returned (representing the current directory).

#### `path.matchesGlob(path, pattern)`

**Added:** v22.5.0 / v20.17.0. **Stabilized (no longer experimental):** v24.8.0
/ v22.20.0.

| Param | Type | Optional |
|---|---|---|
| `path` | `string` | no |
| `pattern` | `string` | no |

Returns: `boolean` — whether `path` matches the glob `pattern`. Throws:
`TypeError` if `path` or `pattern` is not a string. Variant: sync.

Examples: `path.matchesGlob('/foo/bar', '/foo/*')` → `true`;
`path.matchesGlob('/foo/bar*', 'foo/bird')` → `false`. Matching is performed
against the platform's own separator conventions (`path.posix.matchesGlob` vs
`path.win32.matchesGlob` differ on separator handling exactly as the rest of
the module does).

#### `path.normalize(path)`

| Param | Type | Optional |
|---|---|---|
| `path` | `string` | no |

Returns: `string` — `path` with `.`/`..` segments resolved lexically (no
filesystem access — symlinks are never consulted), runs of separators
collapsed to one, and (on Windows) `/` converted to `\`. Throws: `TypeError`
if `path` is not a string. Variant: sync.

Trailing separators are **preserved**. A zero-length string normalizes to
`'.'`.

#### `path.parse(path)`

| Param | Type | Optional |
|---|---|---|
| `path` | `string` | no |

Returns: `ParsedPath` (see §3) — `{ root, dir, base, ext, name }`. Throws:
`TypeError` if `path` is not a string. Variant: sync. Trailing separators are
ignored.

#### `path.relative(from, to)`

| Param | Type | Optional |
|---|---|---|
| `from` | `string` | no |
| `to` | `string` | no |

Returns: `string` — the relative path from `from` to `to`, computed by first
resolving both against the current working directory (as `path.resolve()`
would). If `from` and `to` resolve to the identical path, returns `''`.
Throws: `TypeError` if either argument is not a string. Variant: sync.

Zero-length `from`/`to` are treated as the current working directory. Since
v6.8.0, Windows UNC-path results correctly include the leading slashes.

#### `path.resolve(...paths)`

| Param | Type | Optional |
|---|---|---|
| `...paths` | `string` (variadic, 0..N segments) | yes |

Returns: `string` — an **absolute** path. Throws: `TypeError` if any argument
is not a string. Variant: sync (reads process CWD as its only I/O).

Processes segments **right to left**, prepending each until an absolute path
has been constructed; if the loop exhausts all segments without producing an
absolute path, the process's current working directory is prepended. The
result is normalized (per `normalize()`'s rules) and trailing slashes are
removed (except for a bare root). With zero arguments, returns the absolute
CWD. Zero-length segments are ignored.

#### `path.toNamespacedPath(path)`

**Added:** v9.0.0.

| Param | Type | Optional |
|---|---|---|
| `path` | `string` | no |

Returns: `string`. On Windows: converts an absolute path to the equivalent
[namespace-prefixed](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file#namespaces)
long-path form (`\\?\` for a drive-absolute path, `\\?\UNC\` for a UNC path,
stripping the UNC path's leading double backslash) so filesystem APIs bypass
the legacy `MAX_PATH` (260 char) limit and disable further string
interpretation (no `.`/`..` processing, no separator normalization) by the
underlying Win32 APIs. On POSIX: non-operational — returns `path` unmodified.
If `path` is not a string, it is returned unmodified without throwing (this
function does **not** throw `TypeError`, unlike every other function above —
verify exact non-string behavior against a live Node before finalizing the
native port; documented behavior is "returned without modifications").
Variant: sync.

### Properties & constants

| Property | Type | POSIX value | Windows value |
|---|---|---|---|
| `path.sep` | `string` (constant) | `'/'` | `'\'` |
| `path.delimiter` | `string` (constant) | `':'` | `';'` |
| `path.posix` | `PlatformPath` (object) | — (always the POSIX implementation, on every host) | |
| `path.win32` | `PlatformPath` (object) | — (always the Windows implementation, on every host) | |

- `path.sep` is used for splitting path strings (e.g.
  `'foo/bar/baz'.split(path.sep)`). Note Windows path **parsing** (join,
  normalize, resolve, isAbsolute, …) accepts **both** `/` and `\` as segment
  separators as input, but every `path.*` method that **produces** a path
  string only ever emits `\` (never `/`) on `path.win32`.
- `path.posix.sep === '/'`, `path.posix.delimiter === ':'`,
  `path.win32.sep === '\\'`, `path.win32.delimiter === ';'` — these hold
  regardless of host OS.
- `path.posix.posix === path.posix` and `path.posix.win32 === path.win32`
  (and symmetrically for `path.win32`) — both namespace objects re-expose
  `.posix`/`.win32`/`.sep`/`.delimiter` on themselves, matching Node's actual
  object shape (verify exact self-reference identity, but Node's `path.js`
  does wire `posix.posix = posix; posix.win32 = win32` and vice versa).
- `require('node:path/posix')` / `import posix from 'node:path/posix'` (since
  v15.3.0) gives the exact same object as `require('node:path').posix`, as its
  own top-level module (same for `node:path/win32`).

### Events

None. `node:path` has no `EventEmitter`-based or otherwise event-driven
surface.

## 3. Types & option objects

```typescript
interface ParsedPath {
  /** The root of the path such as '/' or 'C:\' */
  root: string;
  /** The full directory path such as '/home/user/dir' or 'C:\path\dir' */
  dir: string;
  /** The file name including extension (if any), such as 'index.html' */
  base: string;
  /** The file extension (if any), such as '.html' */
  ext: string;
  /** The file name without extension (if any), such as 'index' */
  name: string;
}

interface FormatInputPathObject {
  root?: string;
  dir?: string;
  base?: string;
  ext?: string;
  name?: string;
}

/**
 * The shape of `path`, `path.posix`, and `path.win32` — all three are
 * objects satisfying this same interface (posix/win32 additionally
 * self-reference `.posix`/`.win32` back to the pair).
 */
interface PlatformPath {
  basename(path: string, suffix?: string): string;
  dirname(path: string): string;
  extname(path: string): string;
  format(pathObject: FormatInputPathObject): string;
  isAbsolute(path: string): boolean;
  join(...paths: string[]): string;
  matchesGlob(path: string, pattern: string): boolean;
  normalize(path: string): string;
  parse(path: string): ParsedPath;
  relative(from: string, to: string): string;
  resolve(...paths: string[]): string;
  toNamespacedPath(path: string): string;
  readonly sep: '/' | '\\';
  readonly delimiter: ':' | ';';
  readonly posix: PlatformPath;
  readonly win32: PlatformPath;
}
```

No callback or Promise-shaped signatures exist anywhere in this module — every
function above is a plain synchronous value-returning call.

## 4. Node semantics & edge cases

- **Zero-length-string handling is per-function, not uniform.** `join()` and
  `normalize()` return `'.'` for an empty/all-empty-segments input.
  `resolve()` and `relative()` treat an empty segment/argument as the current
  working directory. `isAbsolute('')` is `false`. `basename('')`, `dirname('')`,
  `extname('')` operate on the literal empty string (verify exact outputs
  against a live Node — the general contract is "no throw, degenerate but
  defined output", e.g. `dirname('')` is documented to return `'.'`).
- **Trailing separators**: ignored by `basename`, `dirname`, `parse` (component
  extraction treats them as insignificant); **preserved** by `normalize`
  (`path.win32.normalize('C:\\temp\\\\foo\\bar\\..\\')` → `'C:\\temp\\foo\\'`,
  note the trailing backslash survives).
- **Repeated/mixed separators are collapsed.** `normalize`/`join`/`resolve`
  collapse runs of separators into one:
  `path.win32.normalize('C:////temp\\\\/\\/\\/foo/bar')` →
  `'C:\\temp\\foo\\bar'`.
- **Windows accepts both `/` and `\` as input separators** in every parsing
  function (join/normalize/resolve/isAbsolute/relative/parse/dirname/
  basename/extname), but every function that **produces** a path only emits
  `\`. POSIX treats `\` as a **literal filename character**, never a
  separator — `path.basename('C:\\foo.html')` on POSIX returns the whole
  string unchanged (`'C:\\foo.html'`) since there's no `/` to split on, while
  the same call under `path.win32` (or on an actual Windows host) returns
  `'foo.html'`.
- **Case sensitivity.** Every path-string operation in this module is
  byte-exact / case-sensitive on **both** platform variants, including
  `path.win32.basename(path, suffix)`'s suffix comparison — this is true even
  though real Windows filesystems are case-insensitive for actual file lookup;
  `node:path` never touches the filesystem, so it has no opportunity (or
  mandate) to normalize case.
- **Windows drive-relative paths vs drive-absolute paths.** `C:foo` (no
  separator after the drive letter) is *drive-relative* — it depends on that
  specific drive's own per-drive current working directory, a legacy DOS/
  Windows concept where each drive letter remembers its own last-used
  directory for the process. `C:\foo` (or `C:/foo`) is *drive-absolute* and
  independent of any working directory. `path.resolve('C:')` and
  `path.resolve('C:\\')` can therefore produce **different** results
  (Node's own docs call this out explicitly) — the former resolves against
  drive `C:`'s remembered working directory, the latter always resolves to
  `C:\`.
- **UNC paths** (`\\server\share\...` or `//server/share/...`) are recognized
  as absolute on `path.win32`; since v6.8.0, `path.relative()` results that
  cross into UNC territory correctly retain their leading slashes (a
  pre-v6.8.0 bug dropped them).
- **`matchesGlob` platform sensitivity.** Because it is separator-aware, a
  glob pattern written with `/` will not match a Windows-style path containing
  `\` under `path.win32.matchesGlob` unless the pattern accounts for it — this
  mirrors the same POSIX-vs-Windows separator duality as every other function.
- **Security: not a path-traversal defense.** Node's docs explicitly warn that
  `path.isAbsolute()`/`path.normalize()`/`path.join()`/`path.resolve()` are
  **not sufficient** to guard against path traversal by themselves — joining
  untrusted `..`-containing input can still lexically escape an intended
  directory boundary; real containment requires comparing the **resolved**
  result against an allow-listed root, plus filesystem-level checks
  (`fs.realpath` + prefix comparison), not string inspection alone.
- **No I/O beyond `resolve()`'s CWD read.** Nothing in this module ever
  touches the filesystem to check existence, follow symlinks, or read
  permissions — this is a pure lexical/string module, and (unlike almost every
  other `node:*` module) has **no error/errno codes** beyond `TypeError` for
  argument-shape violations. No `ENOENT`/`EACCES`/etc. are ever produced here.
- **No backpressure, no ordering guarantees to speak of** — every call is a
  single synchronous computation with no queuing/streaming involved.
- **Deprecations.** None. `node:path` has had no deprecated members through
  Node 25; `matchesGlob` moved from experimental to Stable (2) in v24.8.0/
  v22.20.0, its only stability-status change to date.
- **`node:path/posix` / `node:path/win32` as standalone specifiers** (v15.3.0+)
  are provided so code can `import 'node:path/posix'` without pulling in (or
  caring about) the OS-flavor-dependent default export at all.

## 5. RTS implementation notes

### 5.1 Native impl mapping

`node:path` is **almost entirely implementable as pure `.ts`**, and this is the
recommended design, not a compromise:

- Node's own reference implementation (`lib/path.js` + the internal
  `posix`/`win32` sub-modules) is **100% JavaScript with zero C++ bindings** —
  there is no native `path` binding in Node itself. Every function is a pure
  string/array algorithm operating only on its arguments.
- RTS compiles `.ts` to native Cranelift IR through the same engine pipeline
  as every other user/stdlib module — a `.ts`-shipped port of this algorithm
  is **not an interpretation-overhead compromise** the way it would be in a
  naive "ship JS, run in an interpreter" design. Porting Node's own
  `lib/path.js` line-for-line into a `.ts` shim shipped by `rts-node` gets
  byte-identical behavior "for free" (one algorithm, easy to diff against
  upstream Node for parity bugs) with full native codegen performance.
- The **only genuine native primitive** the `.ts` algorithm cannot supply
  itself is reading the process's **current working directory** (needed by
  `resolve()` when no absolute segment is found), and, on Windows, the
  **per-drive working directory** (needed for the `C:foo` drive-relative
  edge case in §4). Both require an actual OS call, not string manipulation.
- Native impl of those two primitives: `std::env::current_dir()` (Rust std,
  cross-platform) for the plain CWD; the per-drive-CWD primitive is Windows's
  own obscure per-process/per-drive convention (there is no clean public
  Win32 API for it — cmd.exe/Windows itself tracks it via hidden
  per-drive environment variables of the form `=C:`, readable via
  `GetEnvironmentVariableW("=C:", ...)`) — **flagged `(verify)`**, see §7.
- A third tiny native primitive exposes which OS flavor **this compiled
  program's target** actually is (`cfg!(windows)` baked in at the point
  `rts-node` itself is compiled for a given target triple) — this decides
  which algorithm the *default* `path` export (not `.posix`/`.win32`, which
  are always both available) aliases to. This must reflect the **compilation
  target**, not the JIT host, so that `rts compile --target x86_64-pc-windows`
  run on a Linux CI box still produces a `path` default export with Windows
  semantics in the compiled binary.
- No dependency on `std::path::Path`/`PathBuf`: Rust's own path types have
  subtly different normalization/verbatim-prefix/UNC semantics than Node's
  lexical algorithm (e.g. `Path::components()` treats Windows verbatim
  (`\\?\`) prefixes and trailing-slash/dot segments differently), so wrapping
  `std::path` would silently diverge from Node parity in edge cases. The `.ts`
  port mirrors Node's own algorithm directly instead.

### 5.2 ABI surface

Symbol convention: `__RTS_FN_NODE_PATH_<NAME>`. Because the vast majority of
the surface is pure `.ts` (§5.1), the **native surface is deliberately tiny**:

| Symbol | Args (AbiType) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_PATH_CWD` | (none) | `Handle` (GC string) | `std::env::current_dir()`, UTF-8-lossy; used only by the `.ts` `resolve()` algorithm when no absolute segment is found |
| `__RTS_FN_NODE_PATH_WIN32_DRIVE_CWD` | `StrPtr driveLetter` (single ASCII letter, e.g. `"C"`) | `Handle` (GC string) | Windows per-drive working directory (see §5.1, §7 `(verify)`); on non-Windows builds, a portable stub returns the plain process CWD unchanged (no real per-drive concept exists there — matches how the `.win32` variant must still return *something* well-defined when running/compiled on a non-Windows host) |
| `__RTS_FN_NODE_PATH_IS_WIN32` | (none) | `Bool` | compile-time constant (`cfg!(windows)` at the point `rts-node` was built for its target); read once at `.ts` module init to decide whether the default `path` export aliases the `.ts` `win32Impl` or `posixImpl` object |

No `Handle`-table entries beyond the two GC-string returns above are needed —
there is no rich/stateful object anywhere in this module (no class, no
resolver, no open handle to free). All other functions
(`join`/`normalize`/`resolve`'s segment-processing itself/`relative`/`parse`/
`format`/`basename`/`dirname`/`extname`/`isAbsolute`/`toNamespacedPath`/
`matchesGlob`/`sep`/`delimiter`) have **no native symbol at all** — they are
pure `.ts` functions operating on primordial `String`/`Array`/`RegExp` values,
calling only the three symbols above where CWD or target-OS info is needed.

`path.parse()`'s five-field result (`root`/`dir`/`base`/`ext`/`name`) is
constructed entirely in `.ts` as a plain object literal (no native call, no
JSON round-trip needed — unlike modules such as `node:dns` that must marshal
compound results across the ABI boundary, `parse()`'s fields are all derived
from string slicing of the `path` argument that's already resident in `.ts`,
so there is nothing to cross the boundary at all).

`matchesGlob()` is implemented as a `.ts`-side glob-to-`RegExp` compiler
(translate `*`/`**`/`?`/`[...]`/`{...}` into an equivalent regex source string,
then use the **primordial** `RegExp` — native `/re/` syntax support — to test
`path`). This needs no native symbol either, since RegExp is already a
primitive the engine lowers directly (see the PRIMORDIAL-vs-REGISTRY
doctrine).

### 5.3 Async model

None. Every function in `node:path` is **fully synchronous** with no
callback/Promise variant anywhere in the module (Node itself never added
`path.promises`). `resolve()`'s CWD read is a direct blocking `extern "C"`
call (`__RTS_FN_NODE_PATH_CWD`), not routed through the promise subsystem or
the shared tokio runtime — there is no async surface to design here at all,
which makes this one of the simplest modules in the `node:*` set from an
async-model standpoint.

### 5.4 Multithread / worker interaction

- `node:path` carries **no mutable module-level state** of its own — every
  function is a pure function of its arguments, with the sole external input
  being the process's current working directory (and, on Windows, per-drive
  CWDs), both of which are **OS-level, process-global** attributes, not
  RTS-managed heap values. They are not part of the RTS threading model's
  per-thread-region/shared-heap taxonomy at all (no `threadLocal`/`shared`/
  `channel` classification applies) — they are read via a direct blocking
  syscall exactly like any other `std::env` access, and Node/libuv itself
  treats CWD as process-wide (a `process.chdir()` call in one thread is
  visible to `dirname/basename/...`'s CWD-consuming sibling `resolve()` calls
  from every other thread/worker, since `uv_cwd`/`getcwd` are inherently
  process-global on every OS). RTS should preserve this: no per-thread CWD
  isolation is introduced by this module (that would be a **change in Node
  semantics**, not a parity implementation).
- No `Handle`/GC state from this module is ever passed across a
  `worker_threads` `MessagePort`/channel — `path.*` calls never produce a
  value that would need to be (the only two `Handle`-returning natives,
  `CWD`/`WIN32_DRIVE_CWD`, are consumed entirely inside the `.ts` `resolve()`
  algorithm and never surfaced to user code as a raw handle).
- No `SharedArrayBuffer`/shared-memory concerns — no byte data crosses this
  module's boundary at all (see §5.5).

### 5.5 Buffer / TypedArray interop

Not applicable. `node:path` operates exclusively on JS `string` values; no
`Buffer`, `TypedArray`, or `ArrayBuffer` appears anywhere in its signature
surface (unlike `node:fs`, which accepts `Buffer`/path-like unions — that
union-acceptance, if RTS chooses to support `Buffer`-as-path-input for
`node:fs` parity, is `node:fs`'s concern to decode into a `string` **before**
calling into any `path.*` function, not something `node:path` itself needs to
handle).

### 5.6 Doctrine placement

`node:path` is **non-primordial** — it has no native literal/syntactic form
(no `/path/` literal the way `RegExp` has `/re/`), so the engine
(`rts-codegen-new`) must never hardcode `"path"` or any of its member names.
Resolution is purely data-driven, identical in shape to every other `node:`
module already in `rts-node`:

- `import ... from 'node:path'` resolves through
  `rts_node::ns_prefix_for("node:path")` → `"node_path"` (a plain data lookup
  against `NODE_SPECS`, matching the existing mechanism in
  `crates/rts-node/src/lib.rs` — no hardcoded arm in codegen).
- Each native call, e.g. `node_path.cwd()` (used internally by the `.ts`
  `resolve()` shim), resolves via `rts_node::node_lookup("node_path.cwd")` to
  a `NodespaceMember` (`symbol`, `args`, `returns`) exactly like `node:fs`/
  `node:os`/etc.
- `node:path/posix` and `node:path/win32` do **not** need their own
  `NodespaceSpec`/native members (§5.2 established the native surface is
  shared and tiny) — they are resolved as distinct **`.ts`-shim entry
  points** at module-specifier-resolution time (an `rts-node`-owned data
  table mapping the specifier string to which named export of the shared
  `path.ts` shim file to expose as that specifier's default export:
  `"node:path/posix"` → `posixImpl`, `"node:path/win32"` → `win32Impl`,
  `"node:path"` → the OS-flavor-aliased default plus named `posix`/`win32`).
  This mapping still lives entirely in `rts-node`'s own data (the Registry
  equivalent for node modules), never as a codegen-level branch on the string
  `"path"`.
- The native-extern / `.ts`-shim split (§5.1/§5.2): three tiny native
  primitives (`CWD`, `WIN32_DRIVE_CWD`, `IS_WIN32`); everything else — the
  entire `basename`/`dirname`/`extname`/`format`/`isAbsolute`/`join`/
  `matchesGlob`/`normalize`/`parse`/`relative`/`resolve`/`toNamespacedPath`
  algorithm, for both the POSIX and Windows flavors, plus the `sep`/
  `delimiter` constants — lives in a `.ts` shim shipped by `rts-node`
  (`rts-node/src/path/path.ts`, structured as two parameterized
  implementations — `makePlatformPath(sep, isPosix)` called once with
  `('/', true)` and once with `('\\', false)` — mirroring Node's own
  `posix.js`/`win32.js` split).

### 5.7 Shared-infra dependencies (FLAG)

None of the heavy async/tokio/promise/GC-thread-registry infrastructure other
`node:*` modules need applies here. The **only** shared-infra touchpoint is:

- **GC string allocation for the two CWD-returning native calls.** The
  `__RTS_FN_NODE_PATH_CWD`/`__RTS_FN_NODE_PATH_WIN32_DRIVE_CWD` symbols need to
  allocate a GC-tracked string and return a `Handle`. This already lives in
  `rts-engine` itself (`crates/rts-engine/src/heap/handles.rs`:
  `alloc_entry(Entry::String(bytes))` + `read_string_handle(handle)`), which
  is the **lowest layer** in the crate partition — `rts-node` already depends
  on `rts-engine` directly (for `AbiType`, per the existing
  `crates/rts-node/src/path/mod.rs`), so **no hoist is required**: this is
  not a `rts-std`-owned facility being reached around, it is a base-layer API
  `rts-node` can call as an ordinary Rust function.
- **No promise/async subsystem needed** — `node:path` has no async surface at
  all (§5.3).
- **No shared tokio runtime needed** — nothing in this module ever spawns a
  task.
- **No TLS/crypto/net primitives needed.**
- **No GC thread-registry hook needed** — no tokio worker or spawned thread
  is ever created by this module, so there is nothing new to register in
  `gc/thread_registry`.

In short: **none** — this is one of the lightest-weight modules in the
`node:*` surface precisely because it is (almost) pure `.ts`.

### 5.8 Implementation phases

1. **(a)** Add `rts-node/src/path/mod.rs` with the `NodespaceSpec` skeleton
   (`node_module: "path"`, `ns_prefix: "node_path"`) exposing exactly the
   three native members (`cwd`, `winDriveCwd`, `isWin32`); register in
   `NODE_SPECS`. Delete the current thin table in that file (today it
   borrows `__RTS_FN_NS_PATH_*` symbols straight from `rts-std`'s `path`
   namespace — that is the pre-rewrite state this spec replaces, and it
   also has a **pre-existing bug**: its `resolve` member reuses
   `__RTS_FN_NS_PATH_NORMALIZE`'s symbol, which is not resolve semantics at
   all — confirms this module needs a real rewrite, not incremental patching).
2. **(b)** Implement `__RTS_FN_NODE_PATH_CWD` (`std::env::current_dir()` →
   GC string handle) and `__RTS_FN_NODE_PATH_IS_WIN32` (`cfg!(windows)`
   constant). These are the only two primitives needed to make `resolve()`
   and the default-export OS-flavor selection work.
3. **(c)** Write the `.ts` POSIX algorithm (`makePlatformPath('/', true)`):
   `normalize`, `join`, `isAbsolute`, `resolve` (consuming `node_path.cwd()`),
   `relative`, `parse`, `format`, `basename`, `dirname`, `extname`,
   `toNamespacedPath` (no-op passthrough on POSIX). Port directly from
   Node's `lib/path/posix` behavior documented in §2/§4, verified line-by-line
   against the fixture examples already captured in this spec.
4. **(d)** Write the `.ts` Windows algorithm
   (`makePlatformPath('\\', false)`): same function set, handling drive
   letters, drive-relative vs drive-absolute paths (consuming
   `node_path.winDriveCwd()`), UNC paths, dual `/`+`\` input-separator
   acceptance, and `toNamespacedPath`'s real `\\?\`/`\\?\UNC\` prefixing.
5. **(e)** Resolve the §5.1/§7 `(verify)` open question on
   `__RTS_FN_NODE_PATH_WIN32_DRIVE_CWD`'s exact Win32 mechanism (hidden
   per-drive env vars vs another API) and implement it; add the
   non-Windows-host stub fallback.
6. **(f)** Wire the default `path` export (`.ts` picks `win32Impl` or
   `posixImpl` based on `node_path.isWin32()`, called once at module init and
   cached) plus `path.posix`/`path.win32` properties pointing at both
   always-available implementations, with the self-referencing
   `.posix`/`.win32` back-links on each.
7. **(g)** Implement `matchesGlob` as a `.ts` glob→`RegExp` compiler shared by
   both platform implementations (parameterized by separator character).
8. **(h)** Wire `node:path/posix` and `node:path/win32` as distinct
   resolvable specifiers in `rts-node`'s module-specifier table, each
   pointing at the corresponding named export of the same shim file (§5.6).
9. **(i)** Add `path.sep`/`path.delimiter` constants (plain `.ts` literals per
   variant, no native call).

## 6. Test plan

```
tests/node/path/path_basename.test.ts
  - path.basename('/foo/bar/baz/asdf/quux.html') === 'quux.html'
  - path.basename('/foo/bar/baz/asdf/quux.html', '.html') === 'quux'
  - path.win32.basename('C:\\foo.HTML', '.html') === 'foo.HTML' (case mismatch, no strip)
  - path.posix.basename('C:\\temp\\myfile.html') === 'C:\\temp\\myfile.html' (backslash not a separator)
  - path.win32.basename('C:\\temp\\myfile.html') === 'myfile.html'
  - path.basename('') === '' ; path.basename(123 as any) throws TypeError
  - path.basename('/foo/bar///') === 'bar' (trailing separators ignored)

tests/node/path/path_dirname.test.ts
  - path.dirname('/foo/bar/baz/asdf/quux') === '/foo/bar/baz/asdf'
  - path.dirname('foo') === '.'
  - path.dirname('') === '.' (verify)
  - path.dirname(null as any) throws TypeError

tests/node/path/path_extname.test.ts
  - 'index.html' -> '.html'; 'index.coffee.md' -> '.md'; 'index.' -> '.';
    'index' -> ''; '.index' -> ''; '.index.md' -> '.md'
  - path.extname('a/b/.hidden') === ''
  - path.extname('a/b/..hidden') === '.hidden' (verify exact leading-dot edge)

tests/node/path/path_format_parse_roundtrip.test.ts
  - path.parse('/home/user/dir/file.txt') deep-equals
    { root: '/', dir: '/home/user/dir', base: 'file.txt', ext: '.txt', name: 'file' }
  - path.win32.parse('C:\\path\\dir\\file.txt') deep-equals
    { root: 'C:\\', dir: 'C:\\path\\dir', base: 'file.txt', ext: '.txt', name: 'file' }
  - path.format(path.parse(p)) round-trips to an equivalent path for a table of p values (posix + win32)
  - path.format({ root: '/', dir: '/home/user/dir', base: 'file.txt' }) === '/home/user/dir/file.txt' (dir wins over root)
  - path.format({ root: '/', base: 'file.txt', ext: '.ignored' }) === '/file.txt' (base wins over ext/name)
  - path.format({ root: '/', name: 'file', ext: 'txt' }) === '/file.txt' (dot auto-inserted)
  - path.format({ root: '/', name: 'file', ext: '.txt' }) === '/file.txt'

tests/node/path/path_isabsolute.test.ts
  - posix: '/foo/bar' true, '/baz/..' true, 'qux/' false, '.' false
  - win32: '//server' true, '\\\\server' true, 'C:/foo/..' true, 'C:\\foo\\..' true,
    'bar\\baz' false, '.' false, 'C:foo' false (drive-relative, not absolute)
  - path.isAbsolute('') === false
  - path.isAbsolute(42 as any) throws TypeError

tests/node/path/path_join.test.ts
  - path.join('/foo', 'bar', 'baz/asdf', 'quux', '..') === '/foo/bar/baz/asdf'
  - path.join('', 'a', '', 'b') === 'a/b' or 'a\\b' per platform (empty segments skipped)
  - path.join() === '.' ; path.join('') === '.'
  - path.join('a', 1 as any) throws TypeError
  - path.win32.join('C:\\', 'foo', '..\\bar') === 'C:\\bar'

tests/node/path/path_normalize.test.ts
  - path.normalize('/foo/bar//baz/asdf/quux/..') === '/foo/bar/baz/asdf'
  - path.win32.normalize('C:\\temp\\\\foo\\bar\\..\\') === 'C:\\temp\\foo\\'
  - path.win32.normalize('C:////temp\\\\/\\/\\/foo/bar') === 'C:\\temp\\foo\\bar'
  - path.normalize('') === '.'
  - path.normalize('a/b/./c/../../d') === 'a/d'

tests/node/path/path_relative.test.ts
  - path.relative('/data/orandea/test/aaa', '/data/orandea/impl/bbb') === '../../impl/bbb'
  - path.win32.relative('C:\\orandea\\test\\aaa', 'C:\\orandea\\impl\\bbb') === '..\\..\\impl\\bbb'
  - path.relative('/a/b', '/a/b') === ''
  - path.relative('', '/a/b') resolves '' as cwd, still returns a defined string (no throw)
  - path.win32.relative to/from a UNC path retains leading slashes (post v6.8.0 behavior)

tests/node/path/path_resolve.test.ts
  - path.resolve('/foo/bar', './baz') === '/foo/bar/baz'
  - path.resolve('/foo/bar', '/tmp/file/') === '/tmp/file'
  - path.resolve('wwwroot', 'static_files/png/', '../gif/image.gif') starts with cwd and ends '/wwwroot/static_files/gif/image.gif'
  - path.resolve() === current working directory (absolute)
  - path.resolve('') resolves against cwd (empty segment treated as cwd)
  - path.win32.resolve('C:', 'foo') vs path.win32.resolve('C:\\', 'foo') differ (drive-relative vs drive-absolute)

tests/node/path/path_to_namespaced_path.test.ts
  - path.posix.toNamespacedPath('/foo/bar') === '/foo/bar' (no-op)
  - path.win32.toNamespacedPath('C:\\foo\\bar') === '\\\\?\\C:\\foo\\bar'
  - path.win32.toNamespacedPath('\\\\server\\share\\foo') === '\\\\?\\UNC\\server\\share\\foo'
  - path.win32.toNamespacedPath('relative\\path') === 'relative\\path' (non-absolute passthrough, verify)

tests/node/path/path_matches_glob.test.ts
  - path.matchesGlob('/foo/bar', '/foo/*') === true
  - path.matchesGlob('/foo/bar*', 'foo/bird') === false
  - path.matchesGlob('a/b/c', 'a/**/c') === true (globstar, verify exact supported syntax)
  - path.matchesGlob(123 as any, '*') throws TypeError

tests/node/path/path_sep_delimiter_posix_win32.test.ts
  - path.posix.sep === '/'; path.posix.delimiter === ':'
  - path.win32.sep === '\\'; path.win32.delimiter === ';'
  - path.posix.posix === path.posix; path.posix.win32 === path.win32
  - 'foo/bar/baz'.split(path.posix.sep) deep-equals ['foo','bar','baz']
  - 'foo\\bar\\baz'.split(path.win32.sep) deep-equals ['foo','bar','baz']

tests/node/path/path_submodule_imports.test.ts
  - import posix from 'node:path/posix'; posix.sep === '/'
  - import win32 from 'node:path/win32'; win32.sep === '\\'
  - import { posix, win32 } from 'node:path'; posix === require('node:path/posix') (same object identity, verify)

tests/node/path/path_worker_threads.test.ts (multithread)
  - spawn N worker threads, each calling path.resolve('relative/x') concurrently;
    assert every result is prefixed by the SAME process-wide cwd (no per-thread
    CWD drift, matching Node's process-global cwd semantics from §5.4)
  - one worker calls process.chdir(newDir) (via node:process, out of scope here
    but used to validate the interaction); assert path.resolve() in a
    *different* already-running worker subsequently observes the new cwd too
    (process-global, not thread-local) — this test doubles as a regression
    guard against accidentally introducing per-thread CWD isolation for this
    module
```

## 7. Open questions / deferrals

- **Exact native mechanism for `__RTS_FN_NODE_PATH_WIN32_DRIVE_CWD`.** Windows
  per-drive working directories are an obscure, essentially undocumented
  process convention (hidden per-drive environment variables of the form
  `=C:`, historically maintained by `cmd.exe`/the OS loader, readable via
  `GetEnvironmentVariableW("=C:", ...)`). This needs to be validated against
  real Windows behavior (and against what Node itself actually does — Node's
  own handling of `path.resolve('C:')` likely also goes through a similar
  mechanism, or may special-case letting `process.cwd()` stand in when the
  drive matches the process's current drive) before implementation; flagged
  `(verify)` throughout §2/§4/§5.1/§5.2.
- **`path.win32.*` behavior for drive-relative paths when the host/target is
  *not* actually Windows.** Node guarantees `path.win32.*` functions behave
  identically regardless of host OS, but a *real* per-drive CWD concept only
  exists on actual Windows. What should
  `path.win32.resolve('C:', 'foo')` produce when RTS is compiled for
  Linux/macOS? The plan in §5.2 is a stub that falls back to the plain
  process CWD, but this should be checked against Node's actual behavior
  when run cross-platform (e.g. Node for Linux evaluating
  `path.win32.resolve('C:')`) before finalizing — it may already document
  this exact fallback, or it may be genuinely underspecified upstream too.
- **`matchesGlob`'s exact supported glob syntax subset.** The fetched Node
  docs give only two short examples (`/foo/*`, `foo/bird`); the precise
  supported feature set (whether `**` globstar, `{a,b}` brace alternation,
  `[...]` character classes, `!`-negation are supported, and their exact
  matching semantics) needs verification against Node's actual
  `matchesGlob` implementation/tests before the `.ts` glob→`RegExp` compiler
  in §5.2/§5.8(g) can claim full parity — flagged `(verify)` in §2 and the
  test plan.
- **`toNamespacedPath`'s exact non-string-input behavior.** Documented as
  "returned without modifications" rather than throwing `TypeError` like
  every sibling function — this asymmetry should be double-checked against a
  live Node before the `.ts` port bakes in a no-throw special case.
- **`path.posix`/`path.win32` self-reference identity** (`path.posix.win32
  === path.win32`, `path.posix.posix === path.posix`) is asserted from
  general knowledge of Node's `lib/path.js` wiring, not directly confirmed by
  the fetched doc text — worth a quick source-level check (or a live-Node
  `console.log` sanity check) before finalizing the `.ts` shim's object
  construction, since getting this wrong is an easy, cheap-to-avoid parity
  bug.
- **Whether `resolve()`/`join()` should special-case the zero-arg /
  all-zero-length-segment path identically to Node's own edge behavior for
  degenerate inputs** (e.g. `path.join()` with no arguments at all vs
  `path.join('')`) — both should produce `'.'`, but the exact set of
  degenerate-input combinations is worth enumerating exhaustively against
  live Node output during phase (c)/(d) rather than only against the
  examples already captured in this spec.
