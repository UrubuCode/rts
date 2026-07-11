# node:util

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:util` (+ deprecated alias `node:sys`, DEP0025) |
| Node.js version | 25.x (`https://nodejs.org/docs/latest-v25.x/api/util.html`) |
| Stability | 2 - Stable (module-level; `util.diff` experimental→Stable in v25; `util.inherits` is Stability 3 - Legacy; `util._extend`/`util.isArray` are Deprecated — Runtime type, DEP0060/DEP0044) |
| Tier | P0 |
| Status | [ ] Not implemented — spec only |
| Import forms | `import util from 'node:util'`; `import { promisify, callbackify, inspect, format, formatWithOptions, deprecate, inherits, parseArgs, parseEnv, styleText, isDeepStrictEqual, stripVTControlCharacters, toUSVString, debuglog, MIMEType, MIMEParams, types, ... } from 'node:util'`; `const util = require('node:util')`; `const { promisify } = require('node:util')`; deprecated alias `import sys from 'node:sys'` / `const sys = require('node:sys')` (identical surface, warns once via DEP0025) |
| Globals exposed | none directly injected by this module. `util.TextEncoder`/`util.TextDecoder` are plain references to the ambient global `TextEncoder`/`TextDecoder` classes (owned by RTS's web-global infra, not by `node:util` — see §5.1) |

## 1. Purpose

`node:util` is Node's grab-bag of low-level developer-facing utilities: object
introspection/pretty-printing (`inspect`, `format`), callback↔Promise
interop (`promisify`, `callbackify`), type-guards for every JS built-in
(`util.types.*`), a small set of legacy OOP helpers (`inherits`,
`deprecate`), a CLI-arg parser (`parseArgs`) and a `.env`-file parser
(`parseEnv`), terminal styling (`styleText`, `stripVTControlCharacters`),
system-error-code lookups (`getSystemErrorName`/`Map`/`Message`), and the
WHATWG `MIMEType`/`MIMEParams` parser. It has no I/O surface of its own — every
function is either a pure computation over its arguments or a thin
introspection hook into the running process/engine. `node:sys` is kept only
as a deprecated compatibility alias re-exporting the exact same surface.

## 2. Exported API surface (COMPLETE)

### Classes

#### `util.MIMEType` — added v19.1.0 / v18.13.0

Parses and serializes a MIME type string per the WHATWG MIME Sniffing
Standard (not a full RFC 2045 media-type parser — see §4).

| Member | Signature | Notes |
|---|---|---|
| constructor | `new MIMEType(input: string)` | Coerces `input` via `.toString()` first. Throws `TypeError` if the coerced string is not a parseable MIME (exact error code not documented upstream — verify, §7) |
| `mime.type` | getter/setter `string` | Type portion (e.g. `'text'`); setter re-validates and re-serializes |
| `mime.subtype` | getter/setter `string` | Subtype portion (e.g. `'plain'`) |
| `mime.essence` | getter `string` (read-only) | `type/subtype`, no parameters |
| `mime.params` | getter `MIMEParams` (read-only) | Live parameter object — mutating it mutates the owning `MIMEType`'s serialization |
| `mime.toString()` | `(): string` | Full serialized MIME (`type/subtype;param=value;...`) |
| `mime.toJSON()` | `(): string` | Alias of `toString()`, invoked by `JSON.stringify(mime)` |

#### `util.MIMEParams` — added v19.1.0 / v18.13.0

| Member | Signature | Returns | Notes |
|---|---|---|---|
| constructor | `new MIMEParams()` | — | Always starts empty; obtained in practice via `mime.params`, not usually constructed standalone |
| `mimeParams.delete(name)` | `(name: string): void` | — | Removes **all** entries with `name` |
| `mimeParams.entries()` | `(): IterableIterator<[string, string]>` | — | Insertion order |
| `mimeParams.get(name)` | `(name: string): string \| null` | — | First matching value, or `null` |
| `mimeParams.has(name)` | `(name: string): boolean` | — | |
| `mimeParams.keys()` | `(): IterableIterator<string>` | — | |
| `mimeParams.set(name, value)` | `(name: string, value: string): void` | — | Overwrites the **first** existing match; appends if none exists (WHATWG dedupe-on-set rule) |
| `mimeParams.values()` | `(): IterableIterator<string>` | — | |
| `mimeParams[Symbol.iterator]()` | `(): IterableIterator<[string, string]>` | — | Alias of `entries()` |

#### `util.TextDecoder` — reference to the ambient global `TextDecoder`

Not implemented by `node:util` itself; see §5.1/§5.6. Documented surface
(for completeness of this spec, since `node:util` is the surface being
specified):

| Member | Signature | Notes |
|---|---|---|
| constructor | `new TextDecoder(encoding?: string, options?: { fatal?: boolean; ignoreBOM?: boolean })` | `encoding` default `'utf-8'` |
| `textDecoder.encoding` | getter `string` (read-only) | Normalized encoding name |
| `textDecoder.fatal` | getter `boolean` (read-only) | |
| `textDecoder.ignoreBOM` | getter `boolean` (read-only) | |
| `textDecoder.decode(input?, options?)` | `(input?: BufferSource, options?: { stream?: boolean }): string` | |

Supported-encodings set depends on Node's ICU build mode (full-icu /
small-icu / no-icu) — see §4 for the exact list and its RTS implication.

#### `util.TextEncoder` — reference to the ambient global `TextEncoder`

| Member | Signature | Notes |
|---|---|---|
| constructor | `new TextEncoder()` | No arguments |
| `textEncoder.encoding` | getter `string` (read-only) | Always `'utf-8'` |
| `textEncoder.encode(input?)` | `(input?: string): Uint8Array` | `input` default `''` |
| `textEncoder.encodeInto(src, dest)` | `(src: string, dest: Uint8Array): { read: number; written: number }` | Encodes into caller-supplied buffer without allocating |

### Top-level functions

#### `util.callbackify(original)`

| Param | Type | Optional |
|---|---|---|
| `original` | `(...args: any[]) => Promise<any>` | no |

Returns: `(...args: [...any[], (err: Error \| null, value?: any) => void]) => void`.
Throws: `TypeError` if `original` is not a function. Variant: returns a
callback-style function (the callback itself is invoked asynchronously, on
a fresh microtask/tick — never synchronously, even if the input Promise is
already settled). A falsy rejection reason (e.g. `Promise.reject()`) is
wrapped in a fresh `Error` with a `.reason` property holding the original
falsy value, so the callback's `err` argument is never itself falsy.

#### `util.convertProcessSignalToExitCode(signal)`

**Added:** v25.4.0.

| Param | Type | Optional |
|---|---|---|
| `signal` | `string` (e.g. `'SIGTERM'`) | no |

Returns: `number` — `128 + <signal number>` (the POSIX convention for a
process killed by a signal), or `0` if `signal` is not a recognized name
(verify exact fallback, §7). Variant: sync, pure.

#### `util.debuglog(section[, callback])` (alias: `util.debug(section)`)

| Param | Type | Optional | Default |
|---|---|---|---|
| `section` | `string` (supports `NODE_DEBUG` wildcard sections, e.g. `'foo*'`) | no | — |
| `callback` | `(debug: DebuglogFunction) => void` | yes | — (only called once, lazily, the first time logging is actually enabled — an optimization hook so callers can skip expensive argument construction when the section is disabled) |

Returns: `DebuglogFunction` — `(...args: unknown[]) => void` with an
`.enabled: boolean` property. Variant: sync (the returned function writes
to `stderr` synchronously, `util.format`-style, when the `NODE_DEBUG`
environment variable's wildcard pattern matches `section`; a no-op when not
enabled). **`util.debug(section)`** (no second `callback` param in the
common call form) is documented as "an alias for `util.debuglog` without
describing the printed message as a debug message" — **do not confuse this
with the old, removed `util.debug(string)`** which immediately printed a
line to stderr and was eliminated by DEP0028 (End-of-Life, replaced by
`console.error()`); the two share a name but not a signature or era — see
§4.

#### `util.deprecate(fn, msg[, code[, options]])`

| Param | Type | Optional | Default |
|---|---|---|---|
| `fn` | `Function` (plain function or a class) | no | — |
| `msg` | `string` | no | — |
| `code` | `string` (deprecation code, e.g. `'DEP0xxx'`, used to de-duplicate the warning across calls) | yes | — (no code → every call re-warns per §4) |
| `options` | `{ modifyPrototype?: boolean }` | yes | `{ modifyPrototype: true }` |

Returns: `Function` — a wrapper around `fn` that emits a
`process.on('warning')`-visible `DeprecationWarning` (with `.name ===
'DeprecationWarning'`, `.code === code` if given) the first time it is
called, then calls through to `fn` with the same `this`/arguments/return
value every time. `options.modifyPrototype: false` skips copying `fn`'s
prototype onto the wrapper (relevant when `fn` is a class). Variant: sync
wrapper (does not itself change fn's sync/async nature).

#### `util.diff(actual, expected)`

**Added:** v23.11.0; stabilized (no longer experimental) in v25.

| Param | Type | Optional |
|---|---|---|
| `actual` | `Array<any> \| string` | no |
| `expected` | `Array<any> \| string` | no |

Returns: `Array<[operation: -1 \| 0 \| 1, value: string]>` — a Myers
`O(N·D)` diff; `-1` = delete (present only in `actual`), `0` = unchanged
(present in both), `1` = insert (present only in `expected`). Variant:
sync, pure. Throws: `TypeError` on incompatible argument shapes (verify
exact validation, §7).

#### `util.format(format[, ...args])`

| Param | Type | Optional |
|---|---|---|
| `format` | `string` (printf-style; if not a string, all arguments including `format` are inspected+space-joined instead) | yes |
| `...args` | `any[]` | yes |

Returns: `string`. Variant: sync, pure (never throws on well-typed input;
extra args beyond consumed specifiers are space-joined+`util.inspect`-ed
onto the end; a missing arg for a specifier prints the specifier literally,
e.g. `%s` with no more args).

Format specifiers:

| Specifier | Conversion |
|---|---|
| `%s` | String (calls `String()`, or `util.inspect` with `{depth:0, colors:false, compact:3}` for objects with no own `toString`) |
| `%d` | Number via a `parseInt`-like coercion; `BigInt` printed with `n` suffix |
| `%i` | Integer via `parseInt(x, 10)` (drops fractional part) |
| `%f` | Float via `parseFloat` |
| `%j` | JSON (`JSON.stringify`); on a `TypeError` (e.g. circular structure) prints the literal string `'[Circular]'` in place of that argument |
| `%o` | Object — `util.inspect` with `showHidden: true, showProxy: true` (shows non-enumerable properties and Proxy internals) |
| `%O` | Object — `util.inspect` with default options (enumerable-only, no Proxy internals) |
| `%c` | CSS directive — consumed but **ignored** (no terminal effect) |
| `%%` | Literal `%` (consumes no argument) |

#### `util.formatWithOptions(inspectOptions, format[, ...args])`

| Param | Type | Optional |
|---|---|---|
| `inspectOptions` | `InspectOptions` (see §3) | no |
| `format` | `string` | yes |
| `...args` | `any[]` | yes |

Returns: `string`. Identical to `util.format` except any `%o`/`%O`/default
object-stringification step is run through `util.inspect(value,
inspectOptions)` instead of the default options. Variant: sync, pure.

#### `util.getCallSites([frameCount][, options])`

**Added:** v22.9.0.

| Param | Type | Optional | Default |
|---|---|---|---|
| `frameCount` | `integer` (1–200) | yes | `10` |
| `options` | `{ sourceMap?: boolean }` | yes | `{ sourceMap: <enabled iff --enable-source-maps> }` |

Returns: `CallSite[]` (see §3). Throws: `RangeError` if `frameCount` is
out of `[1, 200]` (verify). Variant: sync. Captures the current call stack
independent of any `Error.prepareStackTrace` override (a V8-specific
guarantee — RTS has no V8 stack-trace-callback mechanism at all, so this
guarantee is trivially true but the *mechanism* to produce frames is
entirely different; see §5.1/§7).

#### `util.getSystemErrorName(err)`

| Param | Type | Optional |
|---|---|---|
| `err` | `number` (a Node-API negative error code, **not** a raw OS `errno`) | no |

Returns: `string` (e.g. `'ENOENT'`). Variant: sync, pure lookup. Undefined
behavior (returns `undefined` or throws — verify, §7) for a code with no
mapping.

#### `util.getSystemErrorMap()`

Returns: `Map<number, string>` — every known error code this Node build's
APIs can produce, mapped to its name (`errorMap.get(err.errno)` →
`'ENOENT'`). Variant: sync. Platform-dependent (the set and the codes
themselves come from libuv's per-OS `errno`-normalization table; POSIX and
Windows builds populate different maps because raw OS error numbers
differ, see §4).

#### `util.getSystemErrorMessage(err)`

**Added:** v23.1.0 / v22.12.0.

| Param | Type | Optional |
|---|---|---|
| `err` | `number` | no |

Returns: `string` (human-readable message, e.g. `'No such file or
directory'`). Variant: sync, pure lookup. Platform-dependent (§4).

#### `util.setTraceSigInt(enable)`

**Added:** v24.6.0 / v22.19.0.

| Param | Type | Optional |
|---|---|---|
| `enable` | `boolean` | no |

Returns: `void`. Main-thread-only (no-op or throws on a worker — verify,
§7). Enables/disables printing a stack trace of the currently executing
JS when the process receives `SIGINT`, before the default terminate
behavior. Variant: sync, has a process-wide side effect (installs/removes
a signal handler).

#### `util.inherits(constructor, superConstructor)`

| Param | Type | Optional |
|---|---|---|
| `constructor` | `Function` | no |
| `superConstructor` | `Function` | no |

Returns: `void`. Throws: `TypeError` if either argument is not a function,
or if `superConstructor.prototype` is `undefined`. Variant: sync. Sets
`constructor.prototype = Object.create(superConstructor.prototype)` (with
`constructor` restored as `.constructor`) and `constructor.super_ =
superConstructor`. **Stability: 3 - Legacy** — Node's own docs recommend
ES6 `class ... extends` instead; `node:util` still ships it unconditionally
(not deprecated/warned, just discouraged).

#### `util.inspect(object[, options])` / `util.inspect(object[, showHidden[, depth[, colors]]])`

| Param | Type | Optional |
|---|---|---|
| `object` | `any` | no |
| `options` | `InspectOptions` (see §3) — **or**, in the legacy positional form, `showHidden?: boolean`, `depth?: number \| null`, `colors?: boolean` | yes |

Returns: `string` (capped at roughly 128 MiB of output — beyond that,
truncation/space-exhaustion behavior is implementation-defined, verify
§7). Variant: sync, pure (except for invoking `[util.inspect.custom]`
methods or, with `getters: true`, invoking getters — both of which can run
arbitrary user code with side effects). See §3 for the full
`InspectOptions` shape and §4 for formatting-rule edge cases.

Static members on `util.inspect`:

| Member | Type | Notes |
|---|---|---|
| `util.inspect.custom` | `Symbol` (`Symbol.for('nodejs.util.inspect.custom')`) | Define `obj[util.inspect.custom] = function(depth, options, inspect) { ... }` on any object to fully control its own inspection output (any return type; the return value is itself formatted by `util.inspect`, recursively, unless it's already a string) |
| `util.inspect.defaultOptions` | `InspectOptions` (mutable object) | Overridable process-wide default; consulted by `console.log`, `util.format`'s `%o`/`%O`/fallback path, and every bare `util.inspect(x)` call with no `options` |
| `util.inspect.styles` | `Record<string, string>` | Maps a semantic style name (`bigint`, `boolean`, `date`, `module`, `name`, `null`, `number`, `regexp`, `special`, `string`, `symbol`, `undefined`) to an entry key in `util.inspect.colors`; `regexp`'s value may instead be a function for context-sensitive coloring |
| `util.inspect.colors` | `Record<string, [number, number]>` | ANSI SGR code pairs `[open, close]`. Modifiers: `reset`, `bold`, `italic`, `underline`, `strikethrough` (alias `strikeThrough`), `hidden`, `dim`, `overlined`, `blink`, `inverse`, `doubleunderline`, `framed`. Foreground: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, `gray` (alias `grey`), plus `<color>Bright` variants for each. Background: `bg<Color>` and `bg<Color>Bright` for the same color set |

#### `util.isDeepStrictEqual(val1, val2[, options])`

| Param | Type | Optional | Default |
|---|---|---|---|
| `val1` | `any` | no | — |
| `val2` | `any` | no | — |
| `options` | `{ skipPrototype?: boolean }` | yes | `{ skipPrototype: false }` |

Returns: `boolean`. Variant: sync, pure. Same recursive structural-equality
algorithm as `assert.deepStrictEqual` (see `docs/node-implementation/assert.md`
§3/§4 for the full algorithm this must match byte-for-byte) but returns a
boolean instead of throwing. `skipPrototype: true` allows two objects with
different `[[Prototype]]`s to compare equal if their own enumerable
properties otherwise match.

#### `util.parseArgs([config])`

| Param | Type | Optional | Default |
|---|---|---|---|
| `config` | `ParseArgsConfig` (see §3) | yes | `{}` |

Returns: `ParseArgsResult` (see §3). Throws (when `strict !== false`):
`TypeError` with `code: 'ERR_PARSE_ARGS_UNKNOWN_OPTION'` (unrecognized
`--flag`), `code: 'ERR_PARSE_ARGS_INVALID_OPTION_VALUE'` (e.g. a `boolean`
option given a value, or a `string` option used as a bare flag), and a
generic `TypeError` for malformed `options` config itself (e.g.
`allowNegative: true` on a non-boolean option). Variant: sync, pure (reads
`config.args`, defaulting to `process.argv.slice(2)` — the **only**
implicit external input).

#### `util.parseEnv(content)`

**Added:** v24.4.0.

| Param | Type | Optional |
|---|---|---|
| `content` | `string` (raw contents of a `.env`-format file) | no |

Returns: `Record<string, string>`. Variant: sync, pure. Parses
`KEY=value` lines with the common `.env` dialect (comments via `#`, quoted
values, blank-line skipping, `export KEY=value` prefix tolerance) — exact
quoting/escaping rule set needs verification against Node's own parser
before the `.ts` port claims full parity (§7).

#### `util.promisify(original)`

| Param | Type | Optional |
|---|---|---|
| `original` | `(...args: [...any[], (err: Error \| null, ...values: any[]) => void]) => void` | no |

Returns: `(...args: any[]) => Promise<any>`. Throws: `TypeError` if
`original` is not a function. Variant: returns a Promise-returning
function. If the callback is invoked with more than one non-error value
(`callback(null, a, b)`), the Promise resolves to an array `[a, b]`
(**verify** this exact multi-value tupling behavior, §7); with exactly one
value it resolves to that value directly; with zero it resolves to
`undefined`.

Static member: `util.promisify.custom` (`Symbol`, also
`Symbol.for('nodejs.util.promisify.custom')`) — set
`original[util.promisify.custom] = (...args) => Promise<...>` to fully
override what `util.promisify(original)` returns (used by Node's own
callback-style APIs that have a nonstandard callback signature).

#### `util.stripVTControlCharacters(str)`

| Param | Type | Optional |
|---|---|---|
| `str` | `string` | no |

Returns: `string` (with ANSI/VT100 escape sequences — SGR color codes,
cursor movement, etc. — removed). Variant: sync, pure.

#### `util.styleText(format, text[, options])`

**Added:** v21.7.0 / v20.12.0. **Stabilized:** v23.5.0 / v22.13.0.

| Param | Type | Optional | Default |
|---|---|---|---|
| `format` | `string \| string[]` — one or more names from `util.inspect.colors`, or a hex color `#RGB`/`#RRGGBB` | no | — |
| `text` | `string` | no | — |
| `options` | `{ validateStream?: boolean; stream?: NodeJS.WritableStream }` | yes | `{ validateStream: true, stream: process.stdout }` |

Returns: `string` — `text` wrapped in the ANSI codes for `format`. When
`validateStream: true`, first checks whether `stream` is a color-capable
TTY; if not, returns `text` **unstyled** (no escape codes at all — silent
graceful degradation, not an error). Variant: sync, pure aside from the
stream-capability probe.

#### `util.toUSVString(string)`

| Param | Type | Optional |
|---|---|---|
| `string` | `string` | no |

Returns: `string` — with every lone (unpaired) UTF-16 surrogate code unit
replaced by U+FFFD (REPLACEMENT CHARACTER), producing a valid Unicode
Scalar Value sequence. Variant: sync, pure.

#### `util.transferableAbortController()`

Returns: `AbortController` whose `.signal` is marked transferable (i.e.
includable in a `worker_threads` `postMessage(msg, transferList)` transfer
list) — see §5.4/§7 for the RTS-side mechanism this needs.

#### `util.transferableAbortSignal(signal)`

| Param | Type | Optional |
|---|---|---|
| `signal` | `AbortSignal` | no |

Returns: `AbortSignal` — a transferable wrapper/marking of `signal` for the
same worker-transfer purpose above.

#### `util.aborted(signal, resource)`

| Param | Type | Optional |
|---|---|---|
| `signal` | `AbortSignal` | no |
| `resource` | `object` (any object kept alive only as long as needed to hold the listener — GC hint, not otherwise consulted) | no |

Returns: `Promise<void>` — resolves once `signal` fires `'abort'` (or
immediately/microtask-deferred if already aborted). Variant: returns a
Promise; internally an `AbortSignal` `'abort'`-event listener, no I/O.

### `util.types` namespace — all synchronous, pure, return `boolean`

| Function | Detects |
|---|---|
| `isAnyArrayBuffer(value)` | `ArrayBuffer` or `SharedArrayBuffer` |
| `isArrayBufferView(value)` | Any `TypedArray` or `DataView` |
| `isArgumentsObject(value)` | An `arguments` object |
| `isArrayBuffer(value)` | `ArrayBuffer` (not `SharedArrayBuffer`) |
| `isAsyncFunction(value)` | An `async function` |
| `isBigInt64Array(value)` | `BigInt64Array` |
| `isBigIntObject(value)` | Boxed `BigInt` object (`Object(1n)`) |
| `isBigUint64Array(value)` | `BigUint64Array` |
| `isBooleanObject(value)` | Boxed `Boolean` object (`new Boolean()`) |
| `isBoxedPrimitive(value)` | Any boxed primitive (`Boolean`/`String`/`Number`/`BigInt`/`Symbol` wrapper) |
| `isCryptoKey(value)` | A WebCrypto `CryptoKey` |
| `isDataView(value)` | `DataView` |
| `isDate(value)` | `Date` |
| `isExternal(value)` | A native "External" value (N-API opaque handle) |
| `isFloat16Array(value)` | `Float16Array` |
| `isFloat32Array(value)` | `Float32Array` |
| `isFloat64Array(value)` | `Float64Array` |
| `isGeneratorFunction(value)` | A `function*` |
| `isGeneratorObject(value)` | An object returned by calling a generator function |
| `isInt8Array(value)` | `Int8Array` |
| `isInt16Array(value)` | `Int16Array` |
| `isInt32Array(value)` | `Int32Array` |
| `isKeyObject(value)` | A `node:crypto` `KeyObject` |
| `isMap(value)` | `Map` |
| `isMapIterator(value)` | An iterator returned by `Map.prototype.{keys,values,entries}` |
| `isModuleNamespaceObject(value)` | An ESM `import * as ns` namespace object |
| `isNativeError(value)` | An `Error` produced by the engine itself (survives cross-realm, unlike `instanceof Error`) |
| `isNumberObject(value)` | Boxed `Number` object (`new Number()`) |
| `isPromise(value)` | `Promise` |
| `isProxy(value)` | `Proxy` |
| `isRegExp(value)` | `RegExp` |
| `isSet(value)` | `Set` |
| `isSetIterator(value)` | An iterator returned by `Set.prototype.{values,entries}` |
| `isSharedArrayBuffer(value)` | `SharedArrayBuffer` |
| `isStringObject(value)` | Boxed `String` object (`new String()`) |
| `isSymbolObject(value)` | Boxed `Symbol` object (`Object(Symbol())`) |
| `isTypedArray(value)` | Any `TypedArray` variant |
| `isUint8Array(value)` | `Uint8Array` (includes plain `Buffer`, which extends it) |
| `isUint8ClampedArray(value)` | `Uint8ClampedArray` |
| `isUint16Array(value)` | `Uint16Array` |
| `isUint32Array(value)` | `Uint32Array` |
| `isWeakMap(value)` | `WeakMap` |
| `isWeakSet(value)` | `WeakSet` |

### Deprecated (still present, still callable)

| Function | Signature | DEP code | Type |
|---|---|---|---|
| `util._extend(target, source)` | `(target: object, source: object): object` | DEP0060 | Runtime (warns, still works) — shallow-copies own enumerable properties of `source` onto `target` and returns `target`; replacement: `Object.assign(target, source)` |
| `util.isArray(object)` | `(object: unknown): boolean` | DEP0044 | Runtime (warns, still works) — replacement: `Array.isArray(object)` |

### Removed in earlier majors — DO NOT implement for Node 25 parity

These identifiers **do not exist** on `node:util` as of Node 25 (referencing
them throws `TypeError: util.X is not a function` / `undefined is not a
function`, exactly like any other missing property) — Node's `deprecations.md`
still documents their DEP codes for historical/migration reference, but
implementing them in RTS would be **anti-parity** (RTS `node:util` should
match what a real Node 25 `util` object actually exposes, which is nothing at
these names):

`util.print()` (DEP0026), `util.puts()` (DEP0027), `util.debug(str)` — the
*old*, pre-`debuglog`-alias single-string form (DEP0028; not the same as the
still-current `util.debug(section)` documented above), `util.error()`
(DEP0029), `util.isBoolean()` (DEP0045), `util.isBuffer()` (DEP0046),
`util.isDate()` (DEP0047), `util.isError()` (DEP0048), `util.isFunction()`
(DEP0049), `util.isNull()` (DEP0050), `util.isNullOrUndefined()` (DEP0051),
`util.isNumber()` (DEP0052), `util.isObject()` (DEP0053), `util.isPrimitive()`
(DEP0054), `util.isRegExp()` (DEP0055), `util.isString()` (DEP0056),
`util.isSymbol()` (DEP0057), `util.isUndefined()` (DEP0058), `util.log()`
(DEP0059).

### Properties & constants

| Property | Type | Notes |
|---|---|---|
| `util.inspect.custom` | `Symbol` | See §2 Classes/Functions table above |
| `util.inspect.defaultOptions` | `InspectOptions` (mutable) | See above |
| `util.inspect.styles` | `Record<string,string>` | See above |
| `util.inspect.colors` | `Record<string,[number,number]>` | See above |
| `util.promisify.custom` | `Symbol` | See above |

### Events

None. `node:util` has no `EventEmitter`-based surface; nothing in it emits
events.

## 3. Types & option objects

```typescript
interface InspectOptions {
  /** Include non-enumerable symbols and properties. Default: false. */
  showHidden?: boolean;
  /** Recursion depth; null = unlimited. Default: 2. */
  depth?: number | null;
  /** Emit ANSI color codes. Default: false. */
  colors?: boolean;
  /** Invoke `obj[util.inspect.custom]` when present. Default: true. */
  customInspect?: boolean;
  /** Show a Proxy's [[Target]]/[[Handler]] instead of transparently
   *  forwarding through the proxy traps. Default: false. */
  showProxy?: boolean;
  /** Cap on array/TypedArray/Set/Map entries shown before "... N more
   *  items"; null = unlimited. Default: 100. */
  maxArrayLength?: number | null;
  /** Cap on string length shown before truncation with "...";
   *  null = unlimited. Default: 10000. */
  maxStringLength?: number | null;
  /** Column at which multi-property objects wrap to multiple lines.
   *  Default: 80. */
  breakLength?: number;
  /** false = one line per entry; true = never combine onto one line
   *  beyond depth boundaries; a number N = combine short inner values
   *  up to N per line. Default: 3. */
  compact?: boolean | number;
  /** Sort object keys / Map|Set entries before printing; a function
   *  receives (a, b) => number as a custom comparator. Default: false. */
  sorted?: boolean | ((a: string, b: string) => number);
  /** Inspect getters: 'get' calls only getters, 'set' only setters,
   *  true = both. Default: false (getters not invoked). */
  getters?: boolean | 'get' | 'set';
  /** Add '_' every 3 digits in numeric output (1_000_000). Default: false. */
  numericSeparator?: boolean;
}

/** Passed to a custom `[util.inspect.custom]` method. */
interface InspectOptionsStylized extends InspectOptions {
  stylize(text: string, styleType: string): string;
}

type CustomInspectFunction = (
  this: unknown,
  depth: number,
  options: InspectOptionsStylized,
  inspect: (value: unknown, options?: InspectOptions) => string,
) => unknown;

interface CallSite {
  functionName: string;
  scriptName: string;
  /** Chrome-DevTools-Protocol-style script id (verify RTS analog, §7). */
  scriptId: string;
  /** 1-based. */
  lineNumber: number;
  /** 1-based. */
  columnNumber: number;
}
interface GetCallSitesOptions {
  sourceMap?: boolean;
}

interface DebuglogFunction {
  (...args: unknown[]): void;
  readonly enabled: boolean;
}
type DebuglogOptimizerCallback = (debug: DebuglogFunction) => void;

interface DeprecateOptions {
  /** Default: true. */
  modifyPrototype?: boolean;
}

/** util.parseArgs */
interface ParseArgsOptionConfig {
  type: 'string' | 'boolean';
  /** Default: false. */
  multiple?: boolean;
  /** Single-character alias, e.g. 'f' for '--file'/'-f'. */
  short?: string;
  default?: string | boolean | string[] | boolean[];
}
interface ParseArgsConfig {
  /** Default: process.argv.slice(2). */
  args?: string[];
  options?: Record<string, ParseArgsOptionConfig>;
  /** Default: true. */
  strict?: boolean;
  /** Default: !strict. */
  allowPositionals?: boolean;
  /** Allow a `boolean` option's `--no-<name>` negated form. Default: false. */
  allowNegative?: boolean;
  /** Also return the raw token stream. Default: false. */
  tokens?: boolean;
}
type ParseArgsToken =
  | { kind: 'option'; index: number; name: string; rawName: string;
      value?: string; inlineValue?: boolean }
  | { kind: 'positional'; index: number; value: string }
  | { kind: 'option-terminator'; index: number };
interface ParseArgsResult {
  values: Record<string, string | boolean | (string | boolean)[] | undefined>;
  positionals: string[];
  tokens?: ParseArgsToken[];
}

/** util.styleText */
interface StyleTextOptions {
  /** Default: true. */
  validateStream?: boolean;
  /** Default: process.stdout. */
  stream?: NodeJS.WritableStream;
}

/** util.isDeepStrictEqual */
interface IsDeepStrictEqualOptions {
  /** Default: false. */
  skipPrototype?: boolean;
}
```

No `Promise`-returning signature exists for any function above except
`util.aborted()` (native Promise-returning) and the *products* of
`util.promisify(fn)`/`util.callbackify(fn)`, which convert their argument's
calling convention rather than being async themselves.

## 4. Node semantics & edge cases

- **`util.debug` name collision (real gotcha).** The *current*
  `util.debug(section)` (documented above, an alias of `util.debuglog`) and
  the *old, removed* `util.debug(str)` (DEP0028, an immediate
  `console.error`-equivalent print) are unrelated APIs that happen to share
  a name across Node's history. RTS must implement only the current
  `debuglog`-alias form; do not resurrect the old print-based one.
- **`getSystemErrorName`/`Map`/`Message` operate on *libuv-normalized*
  negative error codes, not raw OS `errno`.** libuv assigns a fixed,
  platform-independent negative integer to each named error (e.g. `ENOENT`
  is always `-2` regardless of host OS), specifically so cross-platform
  Node code can compare `err.errno === -2` reliably. The *raw* OS `errno`
  for "file not found" differs between POSIX (`2`) and is a different
  Win32 error space entirely — libuv's table is the thing actually being
  looked up here, **not** `std::io::Error::raw_os_error()`'s value
  directly. RTS has no libuv dependency, so parity requires embedding (or
  porting) libuv's own name→negative-code table rather than deriving
  numbers from Rust's OS-native errno on each platform (see §5.1/§7).
- **`inspect`'s circular-reference marker** is `<ref *1> ... [Circular
  *1]` (numbered back-references, supporting multiple independent cycles
  in one structure), not a single flat `'[Circular]'` string — that flat
  form is reserved for `util.format('%j', ...)`'s JSON-stringify fallback
  specifically, which is a different (older, simpler) code path.
- **`depth: null` means unlimited**, not "no limit warning" — combined
  with a genuinely cyclic/very-deep structure this can be slow; RTS's `.ts`
  port must still terminate correctly via the circular-reference tracking
  above, independent of the depth counter.
- **`colors: true` forces ANSI codes even to a non-TTY stream** — `inspect`
  itself never auto-detects a terminal; that auto-detection is
  `console.log`'s/`styleText`'s job (`styleText`'s `validateStream` option),
  not `inspect`'s.
- **`getters: true` (or `'get'`) can execute arbitrary user code** (a
  getter can throw, block, or have side effects) — Node's `inspect` catches
  a thrown getter and renders it as an inline error marker rather than
  propagating the exception; this exception-safety wrapper is a real
  behavioral requirement to port, not just a formatting nicety.
- **`compact` interacts with `breakLength` non-trivially**: `compact: 3`
  (the default) means "combine up to 3 short inner entries onto the same
  line as their parent, as long as the combined line does not exceed
  `breakLength`" — it is not simply "show at most 3 items".
- **`%j`'s circular handling is JSON.stringify-based**, independent of
  `inspect`'s own numbered-cycle detector — a circular argument to `%j`
  specifically prints the literal substring `'[Circular]'` (matching what
  `JSON.stringify` itself would throw on, caught and replaced).
- **`util.deprecate`'s warning is deduplicated by `code`, not by `fn`
  identity** — calling `util.deprecate(fn, msg, 'DEP1234')` on two
  different functions with the same code emits the warning only once
  process-wide (for that code); omitting `code` entirely means **every**
  call re-emits its own warning (no dedup key to collapse against).
  Respects the CLI flags `--no-deprecation` (silently skip the warning,
  `fn` still runs), `--trace-deprecation` (attach a full stack trace to
  the warning), `--throw-deprecation` (throw instead of warn, `fn` does
  **not** run) — these are launch-time flags, not environment variables,
  and not settable at runtime after Node/RTS boot (verify RTS's own CLI
  flag equivalents, §7).
- **`parseArgs` strict-mode error codes**: unknown `--flag` →
  `ERR_PARSE_ARGS_UNKNOWN_OPTION`; a `boolean` option given `=value` or a
  `string` option used bare → `ERR_PARSE_ARGS_INVALID_OPTION_VALUE`. A
  positional argument when `allowPositionals` is falsy (the default under
  `strict: true`) is also a hard error under strict mode. `--` always
  terminates option parsing (everything after is positional, emitted as
  an `option-terminator` token when `tokens: true`). Short options can be
  grouped only when every option after the first in the group is itself a
  `boolean` (e.g. `-abc` ⇒ `-a -b -c`, but `-afile` requires `-a`'s type
  to be `string` to consume `file` as its inline value — verify Node's
  exact grouping/value-attachment precedence rules before finalizing the
  `.ts` port, §7).
- **`MIMEType`/`MIMEParams` follow the WHATWG *MIME Sniffing Standard*'s
  parsing algorithm**, which is deliberately looser/different from RFC
  2045/2046 media-type grammar in a few corners (whitespace handling
  around `;`, quoted-string parameter values, case-folding `type`/
  `subtype` to lowercase on parse while leaving parameter *values*
  case-preserved) — port the WHATWG algorithm specifically, not a generic
  MIME/RFC2045 parser, or subtle divergences will appear in edge cases.
- **`MIMEParams.set()` dedupes to the first existing match** — setting a
  parameter name that already appears multiple times overwrites only the
  first occurrence and leaves any duplicates in place (this mirrors
  `URLSearchParams.set()`'s own documented semantics, which RTS's
  `node:url`/`URLSearchParams` spec should already establish precedent
  for — reuse that algorithm rather than reimplementing, §5.1).
- **`toUSVString`** operates on UTF-16 code units (a lone high/low
  surrogate not paired with its counterpart is replaced by U+FFFD); this
  is a straightforward algorithm over `String.prototype.charCodeAt`, which
  the primordial `String` already exposes.
- **TextDecoder's supported-encoding set is ICU-build-dependent** in real
  Node: full-icu gives dozens of legacy encodings (Shift_JIS, GBK, Big5,
  EUC-JP/KR, the ISO-8859-* family, windows-125x, UTF-7, BOCU-1, SCSU,
  …); small-icu gives a curated subset; no-icu gives only UTF-8/UTF-16LE/
  UTF-16BE. RTS's `TextDecoder` (owned outside `node:util`, see §5.1) needs
  its own explicit decision on which encodings it supports — this spec
  does not mandate one, it only notes `node:util`'s re-export must match
  whatever that decision produces (tracked as an open item on
  `TextDecoder`'s own spec, not this one).
- **`node:sys` (DEP0025)** is a pure re-export: importing/requiring it
  warns once (Runtime-type deprecation, suppressible via
  `--no-deprecation`) and then behaves identically to `node:util` in every
  respect — same object identity for every export, not a partial/stale
  copy.
- **Node 25 has genuinely removed, not merely hard-deprecated**, the
  `util.is*`(except `isArray`)/`print`/`puts`/`debug(str)`/`error`/`log`
  family (see the "Removed" table in §2) — a real Node 25 program that
  calls `util.isString(x)` gets a `TypeError: util.isString is not a
  function`. RTS parity means reproducing *that* failure mode (property
  genuinely absent), not silently keeping the old behavior alive under a
  removed name.
- **No error/errno codes originate from `node:util` itself** beyond the
  `TypeError`/`RangeError` argument-validation cases and `parseArgs`'s
  `ERR_PARSE_ARGS_*` codes called out above — this module does no I/O and
  has no filesystem/network/permission error surface of its own.
- **No backpressure/ordering concerns** — every function is a single
  synchronous computation (`aborted()`'s Promise aside, which is a plain
  event-driven resolve with no queue/stream semantics).

## 5. RTS implementation notes

### 5.1 Native impl mapping

`node:util` is overwhelmingly **pure `.ts`**, following the same
philosophy as `node:path` (§5.1 of `docs/node-implementation/path.md`):
Node's own reference implementation of nearly every function here (`format`,
`inspect`, `promisify`, `callbackify`, `inherits`, `deprecate`, `parseArgs`,
`parseEnv`, `styleText`, `stripVTControlCharacters`, `toUSVString`,
`isDeepStrictEqual`, `diff`, `MIMEType`/`MIMEParams`, every `util.types.*`
predicate expressible via `instanceof`/tag-checks against primordial
classes) is plain JavaScript with no native binding, and RTS compiles `.ts`
to native Cranelift IR through the same pipeline as any other code, so a
`.ts` port is full native performance, not an interpretation compromise.

Per-area mapping:

| Area | Implementation | Backing |
|---|---|---|
| `format`/`formatWithOptions`/`inspect` (+ options, colors, styles, custom symbol) | `.ts`, ported from Node's `lib/internal/util/inspect.js` algorithm | Pure string/object-graph algorithm over primordial `Object`/`Array`/`Map`/`Set`/`RegExp`/`Symbol`/`Function` reflection (`Object.getOwnPropertyNames`, `Object.getPrototypeOf`, `Reflect.ownKeys`) — all already primordial/Registry-resolvable |
| `promisify`/`callbackify` | `.ts`, higher-order function wrapping | Primordial `Promise`/`Function` machinery already in the engine — no native call |
| `deprecate` | `.ts` wrapper + one native hook | The wrapper logic (dedup-by-code, wrap/call-through) is `.ts`; **reading which of `--no-deprecation`/`--trace-deprecation`/`--throw-deprecation` is active** is a process-launch-flag read that belongs to `node:process`'s existing CLI-flag surface — reuse it (same crate, ordinary cross-module Rust call inside `rts-node`), do not duplicate a second flag-parsing table |
| `inherits` | `.ts`, `Object.create`+prototype wiring | Pure primordial `Object`/`Function` operations |
| `isDeepStrictEqual` | `.ts`, **shares** the recursive structural-equality algorithm with `node:assert`'s `deepStrictEqual` (see `assert.md` §3/§4) | No native call; reuse, do not duplicate, the assert module's comparator |
| `parseArgs` | `.ts` parser over `process.argv`/`config.args` | Pure string-array algorithm; reads `process.argv` via `node:process`'s existing surface |
| `parseEnv` | `.ts` parser | Pure string algorithm (dotenv-style grammar) |
| `styleText`/`stripVTControlCharacters` | `.ts`, ANSI table + regex | `RegExp` is primordial (native `/re/` syntax) — no native call for stripping; `styleText`'s stream-color-capability probe needs `stream.isTTY`-equivalent, which `node:process`'s stdout/stderr surface already needs to expose for its own parity (reuse, do not re-derive TTY detection here) |
| `toUSVString` | `.ts`, `charCodeAt` scan | Pure primordial `String` operation |
| `diff` | `.ts`, Myers `O(N·D)` algorithm | Pure algorithm over `Array`/`string`, no native call |
| `MIMEType`/`MIMEParams` | `.ts`, WHATWG MIME-Sniffing-Standard parser | Pure string algorithm, structured like `node:url`'s `URLSearchParams` (reuse that module's parameter-list dedupe-on-set logic per §4, if `node:url` lands first) |
| `util.types.*` (most) | `.ts`, `instanceof`/branded-tag checks against primordial classes (`ArrayBuffer`, every `TypedArray`, `DataView`, `Map`, `Set`, `WeakMap`, `WeakSet`, `Promise`, `RegExp`, `Date`, boxed-primitive wrappers) | Primordial/Registry value-model tags already tracked by the engine — a thin `.ts` wrapper, no new native symbol |
| `util.types.isProxy` | `.ts` **if** the engine already exposes a proxy-detection hook; otherwise a **new primordial-level primitive** | `Proxy` is primordial (engine tracks `proxy_parts` internally per CLAUDE.md's doctrine) — detecting "is this value a Proxy" is a value-model concern that belongs at the `rts-adapters`/engine layer, not a `node:util`-specific native (flagged §5.7/§7 if no such hook exists yet) |
| `util.types.isExternal` | Native, N-API-specific | `rts-napi`'s external-handle bookkeeping — a genuine cross-crate dependency (`rts-node` would need a narrow, explicit dependency on `rts-napi`'s handle-tag check, or `rts-napi` publishes the tag check somewhere lower shared crates can see; flagged §5.7/§7) |
| `util.types.isCryptoKey`/`isKeyObject` | `.ts` **brand check** coordinated with `node:crypto` | `node:crypto`'s own `KeyObject`/`CryptoKey` classes (once implemented) must expose a stable, cheap-to-check brand (e.g. a private `Symbol` field or a class-identity check) that `node:util`'s `.ts` shim can import from `node:crypto`'s `.ts` shim — an **intra-`rts-node`** cross-module dependency, not a doctrine violation (both are `rts-node`-owned) |
| `util.types.isModuleNamespaceObject` | Native or engine hook | ESM module-namespace objects are an engine/module-loader-internal concept — needs whatever internal tag the module resolver already stamps on these objects (a new small primitive if none exists yet; flagged §7) |
| `getSystemErrorName`/`Map`/`Message` | Native table (see §5.2) | Requires embedding a **libuv-equivalent negative-error-code table** (§4) — `rts-node` owns this table directly (it is Node-specific, not a general OS-errno concern any other `rts-node` module needs verbatim, though `node:fs`/`node:net`/etc. may want to reuse the *same* table for their own `err.errno`, in which case it should live once in a shared internal module within `rts-node`, not duplicated per-caller-module) |
| `convertProcessSignalToExitCode` | `.ts` lookup table or tiny native, reusing `node:os`'s/`node:process`'s existing signal-name↔-number table | POSIX signal numbers are fixed constants (no OS call needed) — reuse whatever table `node:os`'s `os.constants.signals` already establishes rather than duplicating it |
| `debuglog`/`debug` | `.ts` wildcard-match + one native env read | Wildcard-pattern matching against `NODE_DEBUG` is a pure `.ts` string/regex algorithm; the **environment variable read itself** reuses `node:process`'s existing `process.env` native accessor (cross-module reuse inside `rts-node`, no new native symbol) |
| `getCallSites` | Native, but **fundamentally different mechanism than V8** | RTS has no V8 `CallSite`/`Error.prepareStackTrace` machinery; the nearest analog is a native Cranelift/OS stack walk (e.g. via the `backtrace` crate or platform unwind APIs) producing frame `(function name, source file, line, column)` tuples from RTS's own debug-info emission — **no exact Chrome-DevTools `scriptId` concept exists**, so that field needs an RTS-native substitute (flagged §7, heaviest single open item in this module) |
| `setTraceSigInt` | Native, `SIGINT` handler install/remove | Ties into whatever signal-handling infrastructure `node:process` builds for its own signal surface — reuse, do not stand up a second `SIGINT` hook |
| `transferableAbortController`/`transferableAbortSignal`/`aborted` | `.ts` mostly, one marking hook needed | `AbortController`/`AbortSignal` themselves are **not** implemented by `node:util` (owned by RTS's ambient web-global infra — currently `crates/rts-std/src/globals/abort/`, which is explicitly allowed to keep "web-global infra" per the crate-partition decision); `aborted()` is pure `.ts` (a `Promise` executor + `signal.addEventListener('abort', ...)`); the two `transferable*` functions need a **structured-clone-transferable marking mechanism** that belongs to however `worker_threads`'/`MessagePort`'s transfer-list is eventually designed (not yet specced — flagged §5.7/§7) |
| `TextEncoder`/`TextDecoder` (as re-exported by `util`) | **Not implemented here at all** | `node:util`'s `.ts` shim simply does `export const TextEncoder = globalThis.TextEncoder; export const TextDecoder = globalThis.TextDecoder;` — a plain ambient-global reference, exactly like any ordinary user `.ts` file referencing `console`/`fetch`/`Map` without its containing crate depending on whatever crate implements those. This requires **no** `rts-node` → `rts-std` dependency: the global is already in every program's scope by the time `node:util`'s shim module runs |

### 5.2 ABI surface

Symbol convention: `__RTS_FN_NODE_UTIL_<NAME>`. Because the vast majority of
the surface is pure `.ts` (§5.1), the native surface is small and mostly
reused from sibling `rts-node` modules rather than freshly minted:

| Symbol | Args (AbiType) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_UTIL_SYSTEM_ERROR_COUNT` | (none) | `I32` | Number of entries in the embedded libuv-equivalent error table |
| `__RTS_FN_NODE_UTIL_SYSTEM_ERROR_AT` | `I32 index` | `StrPtr` | Serialized `"<negativeCode>\t<NAME>\t<message>"` for table row `index`; `.ts` splits this once at module init to build the `Map`/lookup objects backing `getSystemErrorName`/`Map`/`Message` (avoids a bespoke rich-object marshalling scheme for a small, load-once table) |
| `__RTS_FN_NODE_UTIL_CONVERT_SIGNAL_EXIT_CODE` | `StrPtr signalName` | `I32` | Thin wrapper reusing `node:os`'s existing signal-number table (§5.1); returns `128 + signalNumber`, or a sentinel (e.g. `0`) for an unrecognized name (verify §7) |
| `__RTS_FN_NODE_UTIL_SET_TRACE_SIGINT` | `Bool enable` | `Void` | Installs/removes the `SIGINT` stack-trace-printing hook; delegates to `node:process`'s signal-handling internals (§5.1) |
| `__RTS_FN_NODE_UTIL_CALLSITE_COUNT` | `I32 maxFrames` | `I32` | Captures the current stack (up to `maxFrames`) into a thread-local buffer and returns how many frames were captured; paired with the four getters below (call-then-drain protocol, avoids allocating a rich per-frame object across the ABI boundary) |
| `__RTS_FN_NODE_UTIL_CALLSITE_FUNCTION_NAME` | `I32 frameIndex` | `StrPtr` | Reads `functionName` for a frame captured by the prior `CALLSITE_COUNT` call |
| `__RTS_FN_NODE_UTIL_CALLSITE_SCRIPT_NAME` | `I32 frameIndex` | `StrPtr` | Reads `scriptName` |
| `__RTS_FN_NODE_UTIL_CALLSITE_LINE_COLUMN` | `I32 frameIndex` | `U64` | Packed `(lineNumber << 32) \| columnNumber`, both 1-based; `.ts` unpacks. `scriptId` is synthesized `.ts`-side from `scriptName` (RTS has no Chrome-DevTools protocol ID concept — see §5.1/§7) |
| `__RTS_FN_NODE_UTIL_ENV_NODE_DEBUG` | (none) | `StrPtr` | Thin passthrough to whatever `node:process`'s own `process.env` getter already exposes for `NODE_DEBUG` — kept as its own symbol only if `node:process`'s generic env-getter isn't directly callable from `.ts` without a key-name marshalling round trip; otherwise **delete this row** and call `node:process`'s existing `env.get("NODE_DEBUG")` `.ts` API directly (prefer the latter — one fewer native symbol) |

No new `Handle`-table entries are needed beyond what already exists
(`MIMEType`/`MIMEParams` instances are ordinary GC-tracked plain objects
with `.ts`-defined shapes, not opaque native handles — they hold no Rust-side
resource). Everything else in §2 (`format`/`inspect`/`promisify`/
`callbackify`/`inherits`/`deprecate`/`parseArgs`/`parseEnv`/`styleText`/
`stripVTControlCharacters`/`toUSVString`/`diff`/`isDeepStrictEqual`/every
`util.types.*` predicate except `isExternal`/`isModuleNamespaceObject`) has
**no native symbol at all**.

`util.types.isProxy`/`isExternal`/`isModuleNamespaceObject`/
`isCryptoKey`/`isKeyObject` are **not** given their own
`__RTS_FN_NODE_UTIL_*` symbols in this table — per §5.1 they resolve
through hooks that, if they don't already exist, belong at a lower layer
(engine/value-model for `isProxy`/`isModuleNamespaceObject`, `rts-napi` for
`isExternal`, `node:crypto`'s own `.ts` surface for
`isCryptoKey`/`isKeyObject`) rather than as fresh `node:util`-specific
natives; see §5.7/§7.

### 5.3 Async model

Overwhelmingly synchronous. The only genuinely async-shaped piece is
`util.aborted(signal, resource)`, which returns a `Promise<void>` driven
purely by an `AbortSignal` `'abort'` event listener — implementable with
the engine's own `Promise` executor primitives (`new Promise((resolve) =>
signal.addEventListener('abort', () => resolve()))`-equivalent `.ts`), with
**no tokio, no thread spawn, no I/O**. `util.promisify`/`util.callbackify`
change a function's *calling convention* (Promise ↔ callback) but do not
themselves perform any I/O or scheduling — whatever `original` does
(sync, callback-async, or genuinely tokio-backed) is untouched by the
wrapper. Every other function in the module (`format`, `inspect`,
`parseArgs`, `getSystemErrorName`, `getCallSites`, …) is a single
synchronous call with no event-loop interaction whatsoever.

### 5.4 Multithread / worker interaction

- **`util.inspect.defaultOptions`/`styles`/`colors` are mutable
  process-visible-looking objects, but Node's real `worker_threads`
  semantics give each Worker its own separate module realm** — a
  `require('node:util')`/`import` inside a Worker gets a **fresh** copy of
  this module's state, not a reference shared with the main thread (a
  mutation to `util.inspect.defaultOptions` in the main thread is **not**
  observed by a Worker, and vice versa). Under the RTS threading model
  (per-thread regions + shared heap with promotion-on-publication), this
  maps cleanly: the `.ts` module-level singleton objects backing these
  three properties are allocated in each thread's **own per-thread
  region**, never promoted to the shared heap — this is the natural,
  zero-extra-effort outcome of treating each thread's module instantiation
  as its own region, and it is also the **correct** Node-parity behavior
  (not a shortcut), so no special-casing is needed here beyond "don't
  explicitly force promotion".
- **`NODE_DEBUG`-driven `debuglog`/`debug` state** (each section's
  `enabled` flag, computed once and cached) is derived from a **process-
  global, read-only-after-boot environment variable**, so it is safe to
  compute once and cache per-thread without any cross-thread
  synchronization concern — an environment variable is immutable for the
  life of the process from RTS's own perspective (nothing in `node:util`
  itself mutates `process.env` at runtime); each thread/Worker recomputing
  its own cached `enabled` flag independently (rather than sharing one
  cache) is both simpler and matches the "fresh module realm per Worker"
  behavior above.
- **`getSystemErrorName`/`Map`/`Message`'s embedded table is immutable,
  read-only static data** — trivially safe to share across every thread
  with zero synchronization (no `Mutex`/`RwLock` needed, just a `const`/
  `static` table baked in at compile time), unlike most other `rts-node`
  state.
- **`MIMEType`/`MIMEParams` instances are ordinary per-object heap state**
  with no thread-affinity of their own; if passed across a
  `worker_threads` `MessagePort`, Node's real behavior is that they are
  **not** structured-clone-registered types, so `postMessage(mimeInstance)`
  throws `DataCloneError` — RTS should reproduce this by simply *not*
  registering `MIMEType`/`MIMEParams` on whatever structured-clone
  allow-list `worker_threads` eventually builds (natural non-support,
  not a feature to add).
- **`transferableAbortController`/`transferableAbortSignal` are the one
  genuine coupling point to the RTS threading model.** Their entire
  purpose is enabling an `AbortSignal` to be named in a `MessagePort`
  `transferList` so a Worker can observe (or trigger) an abort originating
  in a different thread/region — this requires whatever `worker_threads`'
  MessagePort-transfer design settles on to include an explicit allow-list
  entry (or a generic "transferable" marker interface) for `AbortSignal`/
  `AbortController`, analogous to how `ArrayBuffer`/`MessagePort` are
  transferable. This module cannot finalize its own implementation of
  these two functions until that mechanism exists elsewhere — flagged as
  a genuine cross-module blocker in §5.7/§7, not merely a nice-to-have.
- No other part of `node:util` holds any state that needs `threadLocal`/
  `shared`/`channel` classification — everything else is either a pure
  function of its arguments or the process-global read-only data above.

### 5.5 Buffer / TypedArray interop

- `util.TextEncoder.prototype.encode`/`encodeInto` produce/consume
  `Uint8Array` — but, per §5.1, `TextEncoder` itself is **not**
  implemented by `node:util`; this module only re-exports the ambient
  global reference, so it inherits whatever byte-crossing mechanism that
  global class already uses (primordial `TypedArray`/engine-owned memory
  model — no separate marshalling path needed here).
- `util.types.isTypedArray`/`isUint8Array`/`isInt32Array`/etc. inspect the
  primordial value-model's existing type tag on a `TypedArray` value
  directly (the engine already distinguishes `Repr`/tag information for
  these per the PolyValue/shape model) — a thin `.ts` predicate wrapper,
  no bytes actually cross any ABI boundary for a type-guard check.
- No other function in `node:util` accepts or produces `Buffer`/
  `TypedArray`/`ArrayBuffer`/`DataView` data. `inspect()`'s special
  pretty-printing of a `Buffer`/`TypedArray` argument (Node prints
  `Uint8Array(4) [ 1, 2, 3, 4 ]`-style output, truncated at
  `maxArrayLength`) reads the elements the same way it reads any other
  indexed collection — no distinct native path.

### 5.6 Doctrine placement

`node:util` is **non-primordial** — it has no native literal/syntactic
form, so the engine (`rts-codegen-new`) must never hardcode `"util"` or
any of its member names. Resolution is fully data-driven, matching the
existing mechanism already established for `node:fs`/`node:path`/`node:os`/
etc. in `crates/rts-node/src/lib.rs`:

- `import ... from 'node:util'` resolves through
  `rts_node::ns_prefix_for("node:util")` → `"node_util"` (a data lookup
  against `NODE_SPECS`, no codegen-level branch on the string `"util"`).
- Each remaining native call (the small §5.2 table) resolves via
  `rts_node::node_lookup("node_util.<name>")` → a `NodespaceMember`
  exactly like every other `node:*` module.
- **`node:sys` as a data-table alias, not a codegen special case.** The
  current `NodespaceSpec`/`ns_prefix_for`/`node_lookup` trio (see
  `crates/rts-node/src/lib.rs`) matches a specifier by exact
  `node_module` field equality (`specifier.strip_prefix("node:")` then a
  linear `find` over `NODE_SPECS`), which has **no alias concept today**.
  Adding `node:sys` support means extending `NodespaceSpec` with a small
  `aliases: &'static [&'static str]` field (or a separate flat
  `ALIAS_TABLE: &[(&str, &str)]` consulted before the primary lookup) so
  `"sys"` resolves to the **same** `util::SPEC`'s `ns_prefix` — a second
  row of data, never a hardcoded `if specifier == "node:sys"` arm in
  codegen or even in `rts-node`'s resolver function bodies. This mirrors
  exactly how `node:path/posix`/`node:path/win32` are handled as distinct
  resolvable specifiers pointing at shared underlying implementation
  (`path.md` §5.6).
- The native-extern/`.ts`-shim split (§5.1/§5.2): a handful of small
  native primitives (system-error table, signal-to-exit-code, `SIGINT`
  trace toggle, call-site capture) plus reuse of sibling modules'
  existing natives (`node:process`'s env/TTY/argv/signal surface,
  `node:os`'s signal-number table); everything else — `format`/`inspect`/
  `promisify`/`callbackify`/`inherits`/`deprecate`/`parseArgs`/`parseEnv`/
  `styleText`/`stripVTControlCharacters`/`toUSVString`/`diff`/
  `isDeepStrictEqual`/`MIMEType`/`MIMEParams`/nearly all of `util.types.*`
  — lives in one or more `.ts` shims shipped by `rts-node`
  (`rts-node/src/util/util.ts` + a `types.ts` sub-shim for the `util.types`
  namespace).

### 5.7 Shared-infra dependencies (FLAG)

- **`node:process`'s env/CLI-flag/TTY/argv/signal-handling surface.**
  `debuglog`'s `NODE_DEBUG` read, `deprecate`'s `--no-deprecation`/
  `--trace-deprecation`/`--throw-deprecation` flag checks, `parseArgs`'s
  default `process.argv` source, `styleText`'s stream-TTY-capability
  probe, and `setTraceSigInt`'s `SIGINT` handler installation all need
  primitives that properly belong to (or are already being specced for)
  `node:process`. This is an **intra-`rts-node` cross-module dependency**,
  not a `rts-std` dependency — both modules live in the same crate — but
  it must be sequenced: `node:process`'s relevant natives should land
  before (or alongside) these specific `node:util` functions, or this
  module will need short-lived duplicate stubs.
- **`node:os`'s signal-name↔-number table**, needed by
  `convertProcessSignalToExitCode`. Same intra-crate reuse point as above.
- **A `node:crypto`-owned `KeyObject`/`CryptoKey` brand check**, needed by
  `util.types.isKeyObject`/`isCryptoKey`. Depends on `node:crypto`'s own
  class design (not yet finalized in this doc set); flag as a
  cross-module sequencing dependency, not a blocker to shipping the rest
  of `util.types`.
- **`rts-napi`'s external-handle tag**, needed by `util.types.isExternal`.
  A genuine cross-crate touchpoint (`rts-node` ↔ `rts-napi`) — if
  `rts-napi` doesn't already expose a cheap "is this an External" check
  usable from `rts-node`, one needs to be added there (owned by
  `rts-napi`, not duplicated in `rts-node`).
- **An engine/`rts-adapters`-level Proxy-detection primitive**, needed by
  `util.types.isProxy`. Per the PRIMORDIAL-vs-REGISTRY doctrine, `Proxy`
  is primordial and the engine already tracks `proxy_parts` internally for
  its own `get`/`set`/`delete` trap dispatch (per CLAUDE.md); whether that
  internal tracking is *already* exposed as a queryable "is-proxy" fact
  needs verification — if not, this is a small addition at the
  `rts-adapters`/engine layer (not `rts-node`-owned code, since it's a
  value-model concern), which `node:util`'s `.ts` shim would then call
  through whatever the engine's existing Proxy-primitive access pattern is
  (not a new `rts-std`-shaped dependency — `rts-node` may call into
  `rts-engine`/`rts-adapters` directly, the same way it already depends on
  `rts_engine::abi::AbiType` today).
- **An engine/module-loader-level ESM-namespace-object tag**, needed by
  `util.types.isModuleNamespaceObject`. Similarly a module-system-internal
  concern rather than a `node:util`-owned implementation detail.
- **A `worker_threads`-owned structured-clone-transferable marking
  mechanism**, needed by `transferableAbortController`/
  `transferableAbortSignal`. This is the module's single heaviest
  external dependency: `node:util` cannot fully implement these two
  functions in isolation — they need `worker_threads`' eventual
  `MessagePort`/transfer-list design (not yet specced anywhere in this doc
  set) to define what "marking a value transferable" even means
  mechanically in RTS. Until that lands, these two functions can be
  stubbed as identity functions (`transferableAbortSignal(s) === s`) that
  produce a *correct-looking* `AbortController`/`AbortSignal` but do not
  yet actually behave differently across a real worker transfer — flagged
  explicitly, not silently shipped as "done".
- **`AbortController`/`AbortSignal`'s own implementation**, needed by
  `aborted()`/`transferableAbortController()`. Not owned by `node:util` —
  currently lives in `crates/rts-std/src/globals/abort/` as ambient
  web-global infra (an allowed `rts-std` surface per the crate-partition
  decision: "rts-std keeps only RTS-unique surface (audio/asio_audio/ui)
  and the web-global infra"). `node:util`'s `.ts` shim references the
  ambient global directly (no crate dependency, see §5.1's `TextEncoder`/
  `TextDecoder` treatment) — flagged here only so a future reader doesn't
  mistake "the global exists" for "`node:util` implements it".

If none of the above existed and had to be built from scratch inside
`rts-node` with zero reuse, that would be the honest worst case; in
practice most of these are small, already-motivated additions to sibling
`node:*` modules this spec set is building anyway.

### 5.8 Implementation phases

1. **(a)** Add the real `NodespaceSpec` skeleton in
   `rts-node/src/util/mod.rs` (`node_module: "util"`, `ns_prefix:
   "node_util"`), replacing the current placeholder (today's
   `crates/rts-node/src/util/mod.rs` exposes only non-Node-shaped
   `formatInt`/`formatFloat`/`formatHex`/`formatBin`/`formatOct`/
   `parseInt`/`parseFloat` wrappers borrowed from `rts::fmt` — a
   pre-rewrite stub, not real `util` surface, to be deleted wholesale).
   Extend `NodespaceSpec`/`ns_prefix_for`/`node_lookup` in
   `crates/rts-node/src/lib.rs` with the alias mechanism from §5.6 and
   register `"sys"` as an alias of `"util"`.
2. **(b)** Implement the tiny native primitives that have no `.ts`-only
   alternative: the embedded system-error table
   (`SYSTEM_ERROR_COUNT`/`SYSTEM_ERROR_AT`, §5.2) — this unblocks
   `getSystemErrorName`/`Map`/`Message` — and confirm/port libuv's actual
   name→negative-code→message table (§4/§7) as the source data.
3. **(c)** Write the `.ts` `format`/`formatWithOptions` port (printf-style
   specifiers table, §2), independent of `inspect`'s full recursive
   algorithm (using a depth-0/shallow stringify fallback initially).
4. **(d)** Write the `.ts` `inspect` port: recursive object-graph walk,
   circular-reference numbered-cycle tracking, `depth`/`maxArrayLength`/
   `maxStringLength`/`breakLength`/`compact`/`sorted`/`getters`/
   `numericSeparator` option handling, `util.inspect.custom` invocation,
   `defaultOptions`/`styles`/`colors` static objects. This is the single
   largest `.ts` port in the module — budget the most implementation time
   here.
5. **(e)** Wire `format`'s `%o`/`%O` specifiers through the now-complete
   `inspect` (step d depends on step c's specifier table but the full
   object-formatting path depends on d completing first — sequence
   accordingly).
6. **(f)** `promisify`/`callbackify`/`inherits`/`deprecate` (deprecate's
   wrapper logic first, its flag-reading dependency on `node:process`
   second per §5.7) — independent, straightforward `.ts` ports.
7. **(g)** `parseArgs` (full strict-mode validation + `tokens` output) and
   `parseEnv` — independent of steps c/d.
8. **(h)** `styleText`/`stripVTControlCharacters` (ANSI table + regex),
   deferring the TTY-capability probe until `node:process`'s stream
   surface exposes it (§5.7); ship with `validateStream: false`-equivalent
   behavior as an interim if needed, flagged.
9. **(i)** `isDeepStrictEqual`, sharing the algorithm with `node:assert`
   (sequence after/alongside `assert.md`'s own implementation phases if
   that module lands first; otherwise implement the shared algorithm once
   in a location both modules can import from within `rts-node`).
10. **(j)** `diff` (Myers algorithm, self-contained).
11. **(k)** `MIMEType`/`MIMEParams` (WHATWG parser + live-params object).
12. **(l)** `util.types.*` — implement the `instanceof`/tag-check-based
    majority first (all `TypedArray`/`ArrayBuffer`/`Map`/`Set`/`Promise`/
    `RegExp`/`Date`/boxed-primitive/generator/async-function checks);
    leave `isProxy`/`isExternal`/`isModuleNamespaceObject`/`isCryptoKey`/
    `isKeyObject` for last, each gated on its respective §5.7 dependency.
13. **(m)** `toUSVString` (self-contained).
14. **(n)** `getCallSites` — the heaviest native item (§5.1/§7); implement
    only after confirming RTS's own stack-walk/debug-info mechanism can
    produce the needed `(functionName, scriptName, line, column)` tuples;
    synthesize `scriptId` as a stable per-file hash or index in the
    interim (flagged, not a real Chrome-DevTools ID).
15. **(o)** `convertProcessSignalToExitCode`/`setTraceSigInt`, reusing
    `node:os`'s signal table and `node:process`'s signal-handling hook
    respectively (§5.7 sequencing).
16. **(p)** `transferableAbortController`/`transferableAbortSignal`/
    `aborted` — `aborted()` first (self-contained given `AbortSignal`
    already exists as an ambient global), the two `transferable*`
    functions last, stubbed per §5.7 until `worker_threads`' transfer-list
    mechanism exists.
17. **(q)** Deprecated-but-present `_extend`/`isArray` (trivial, low
    priority — implement whenever convenient, they carry no design risk).

## 6. Test plan

```
tests/node/util/util_format.test.ts
  - util.format('%s:%s', 'a', 'b') === 'a:b'
  - util.format('%d apples', 5) === '5 apples'; util.format('%d', 'abc') is NaN-string per parseInt-like coercion
  - util.format('%i', 5.9) === '5' (truncates); util.format('%f', '3.14abc') === '3.14'
  - util.format('%j', { a: 1 }) === '{"a":1}'; circular object -> '[Circular]'
  - util.format('%o', { a: 1 }) includes non-enumerable marker vs util.format('%O', {a:1}) does not
  - util.format('%%') === '%'
  - util.format('%s', 'a', 'b', 'c') === 'a b c' (extra args space-joined+inspected)
  - util.format('no specifiers', 1, 2) === 'no specifiers 1 2'
  - util.format({a:1}) with non-string first arg inspects+joins all args

tests/node/util/util_format_with_options.test.ts
  - util.formatWithOptions({ colors: true }, '%o', {a:1}) contains ANSI escape codes
  - util.formatWithOptions({ depth: 0 }, '%o', { a: { b: { c: 1 } } }) truncates nested object at depth 0

tests/node/util/util_inspect_basic.test.ts
  - util.inspect(42) === '42'; util.inspect('str') === "'str'"; util.inspect(null) === 'null'
  - util.inspect([1,2,3]) === '[ 1, 2, 3 ]'
  - util.inspect({a:1,b:2}) === '{ a: 1, b: 2 }'
  - util.inspect(new Map([['a',1]])) contains 'Map(1)'
  - util.inspect(new Set([1,2])) contains 'Set(2)'

tests/node/util/util_inspect_circular.test.ts
  - const o: any = {}; o.self = o; util.inspect(o) matches /<ref \*1>.*\[Circular \*1\]/s
  - two independent cycles in one object produce two distinct ref numbers

tests/node/util/util_inspect_depth_and_length.test.ts
  - util.inspect({ a: { b: { c: { d: 1 } } } }, { depth: 1 }) truncates below depth 1 as '[Object]'
  - util.inspect({ a: { b: { c: 1 } } }, { depth: null }) shows full nesting
  - util.inspect(Array.from({length:200}, (_,i)=>i), { maxArrayLength: 10 }) shows "... 190 more items"
  - util.inspect('x'.repeat(20000), { maxStringLength: 5 }) truncates with '...'

tests/node/util/util_inspect_custom.test.ts
  - class Foo { [util.inspect.custom]() { return 'CUSTOM'; } }; util.inspect(new Foo()) === 'CUSTOM'
  - custom function receives (depth, options, inspect) and can recursively call inspect(this.inner, options)
  - util.inspect(new Foo(), { customInspect: false }) ignores the custom method, shows default shape

tests/node/util/util_inspect_getters_and_showhidden.test.ts
  - object with a throwing getter: util.inspect(obj, { getters: true }) does not throw, shows an error marker
  - non-enumerable property shown only with showHidden: true

tests/node/util/util_inspect_sorted_and_numeric_separator.test.ts
  - util.inspect({ b: 1, a: 2 }, { sorted: true }) shows 'a' before 'b'
  - util.inspect({ b:1, a:2 }, { sorted: (a,b) => b.localeCompare(a) }) reverses order
  - util.inspect(1000000, { numericSeparator: true }) === "1_000_000"

tests/node/util/util_inspect_colors_and_styles.test.ts
  - util.inspect(1, { colors: true }) contains ANSI codes matching util.inspect.colors.number
  - mutating util.inspect.styles.number to a different color key changes subsequent output

tests/node/util/util_promisify.test.ts
  - promisified fs-like callback fn resolves with the single callback value
  - callback invoked with (null, a, b) -> resolves to [a, b] (verify exact tupling, see §7)
  - callback invoked with an Error -> promise rejects with that Error
  - fn with util.promisify.custom defined -> promisify(fn) returns exactly that custom function
  - util.promisify(notAFunction) throws TypeError

tests/node/util/util_callbackify.test.ts
  - callbackify(async (x) => x*2)(5, (err, val) => expect(val).toBe(10))
  - callbackify(async () => { throw new Error('boom'); })((err) => expect(err.message).toBe('boom'))
  - callbackify(() => Promise.reject(false))((err) => expect(err instanceof Error && err.reason === false).toBe(true))
  - callback is invoked asynchronously even for an already-resolved promise (never synchronously)

tests/node/util/util_inherits.test.ts
  - function Base(){} function Derived(){} util.inherits(Derived, Base);
    new Derived() instanceof Base === true; Derived.super_ === Base
  - util.inherits(1 as any, Base) throws TypeError
  - util.inherits(Derived, {} as any) throws TypeError (no .prototype)

tests/node/util/util_deprecate.test.ts
  - deprecated fn warns exactly once when called twice with the SAME code
  - two different deprecated fns sharing the SAME code -> warning fires once total across both
  - two deprecated fns with DIFFERENT (or no) code each warn independently
  - deprecated fn still returns fn's real return value / forwards `this` and arguments correctly
  - deprecate(Cls, msg, code, { modifyPrototype: false }) leaves the wrapper's prototype untouched

tests/node/util/util_parse_args_basic.test.ts
  - parseArgs({ args: ['--foo', 'bar'], options: { foo: { type: 'string' } } })
    .values.foo === 'bar'
  - parseArgs({ args: ['--flag'], options: { flag: { type: 'boolean' } } })
    .values.flag === true
  - parseArgs({ args: ['-f','x'], options: { file: { type:'string', short:'f' } } })
    .values.file === 'x'
  - multiple: true collects repeated flags into an array
  - default value used when flag absent

tests/node/util/util_parse_args_strict_and_positionals.test.ts
  - unknown flag under strict:true throws with code ERR_PARSE_ARGS_UNKNOWN_OPTION
  - boolean option given '=value' throws ERR_PARSE_ARGS_INVALID_OPTION_VALUE
  - allowPositionals:true collects non-flag args into .positionals
  - '--' terminates option parsing; everything after is positional
  - allowNegative:true + boolean option supports '--no-foo' setting false
  - tokens:true returns the raw token array with kind/index/name/value shapes matching §3

tests/node/util/util_parse_env.test.ts
  - parseEnv('FOO=bar\nBAZ=qux') deep-equals { FOO: 'bar', BAZ: 'qux' }
  - comment lines (# ...) and blank lines ignored
  - quoted value with embedded '=' preserved: KEY="a=b" -> { KEY: 'a=b' }
  - export-prefixed line 'export FOO=bar' parses same as 'FOO=bar' (verify against real Node)

tests/node/util/util_style_text_and_strip.test.ts
  - util.styleText('red', 'hi') contains the ANSI code pair from util.inspect.colors.red
  - util.styleText(['bold','red'], 'hi') applies both codes
  - util.styleText('#ff0000', 'hi') applies a hex-derived color
  - util.styleText('red', 'hi', { validateStream: true, stream: nonTtyStream }) returns 'hi' unstyled
  - util.stripVTControlCharacters(util.styleText('red','hi')) === 'hi'

tests/node/util/util_to_usv_string.test.ts
  - util.toUSVString('a\uD800b') replaces the lone surrogate with U+FFFD
  - a well-formed surrogate pair (e.g. an emoji) passes through unchanged

tests/node/util/util_is_deep_strict_equal.test.ts
  - isDeepStrictEqual({a:1,b:[1,2,{c:3}]}, {a:1,b:[1,2,{c:3}]}) === true
  - isDeepStrictEqual(new Foo(1), new Bar(1)) === false by default;
    === true with { skipPrototype: true } when own-enumerable fields match
  - isDeepStrictEqual(NaN, NaN) === true (SameValue semantics, unlike ===)
  - isDeepStrictEqual(1, '1') === false (strict, no coercion)

tests/node/util/util_diff.test.ts
  - util.diff('abc', 'abd') has a delete of 'c' and an insert of 'd', 'a'/'b' unchanged
  - util.diff([1,2,3], [1,2,3]) is all-unchanged entries
  - util.diff([], ['x']) is a single insert entry

tests/node/util/util_mimetype.test.ts
  - new MIMEType('text/plain; charset=utf-8').essence === 'text/plain'
  - mime.params.get('charset') === 'utf-8'; mime.params.set('charset','ascii'); mime.toString() reflects it
  - mime.params.set('newparam','v'); has two params now, iterable via entries()/keys()/values()
  - new MIMEType('not a mime') throws TypeError
  - JSON.stringify(mime) === '"' + mime.toString() + '"' (via toJSON)

tests/node/util/util_types_predicates.test.ts
  - util.types.isTypedArray(new Uint8Array()) === true; isTypedArray([]) === false
  - util.types.isArrayBuffer(new ArrayBuffer(4)) === true; isAnyArrayBuffer(sharedArrayBuffer) === true
  - util.types.isPromise(Promise.resolve()) === true
  - util.types.isMap(new Map()) === true; isMapIterator(new Map().keys()) === true
  - util.types.isRegExp(/x/) === true; isDate(new Date()) === true
  - util.types.isAsyncFunction(async()=>{}) === true; isGeneratorFunction(function*(){}) === true
  - util.types.isBoxedPrimitive(new Number(1)) === true; isBoxedPrimitive(1) === false
  - util.types.isNativeError(new TypeError()) === true

tests/node/util/util_types_proxy_and_native_error.test.ts
  - util.types.isProxy(new Proxy({}, {})) === true; isProxy({}) === false
  - util.types.isNativeError(new Error()) === true across a value produced by JSON.parse-then-throw path too

tests/node/util/util_get_system_error.test.ts
  - util.getSystemErrorName(-2) === 'ENOENT' (assuming -2 is RTS's embedded ENOENT code, verify against fs error path)
  - util.getSystemErrorMessage(-2) is a non-empty human-readable string
  - util.getSystemErrorMap() is a Map whose .get(-2) === 'ENOENT'
  - triggering a real fs.readFileSync('missing') error and reading its own .errno through getSystemErrorName
    round-trips to the same name fs itself reports (cross-module consistency check with node:fs)

tests/node/util/util_convert_signal_exit_code.test.ts
  - util.convertProcessSignalToExitCode('SIGTERM') === 128 + <SIGTERM number>
  - util.convertProcessSignalToExitCode('NOT_A_SIGNAL') returns the documented fallback (verify, currently 0)

tests/node/util/util_debuglog.test.ts
  - with NODE_DEBUG unset: util.debuglog('foo').enabled === false, calling it writes nothing to stderr
  - with NODE_DEBUG=foo: util.debuglog('foo').enabled === true, calling it writes a formatted line to stderr
  - with NODE_DEBUG=foo*: util.debuglog('foobar').enabled === true (wildcard match)
  - util.debug === util.debuglog behaviorally for the single-arg form

tests/node/util/util_deprecated_and_removed.test.ts
  - util._extend({a:1},{b:2}) deep-equals {a:1,b:2} (still works, deprecated)
  - util.isArray([1,2]) === true (still works, deprecated)
  - (util as any).isString is undefined (removed in Node 25 - regression guard against
    accidentally resurrecting a DEP0056-era helper)
  - (util as any).log is undefined (DEP0059 removed)

tests/node/util/util_sys_alias.test.ts
  - import sys from 'node:sys'; sys.format === (await import('node:util')).format (same function reference)
  - importing 'node:sys' emits exactly one DeprecationWarning (DEP0025) per process, not per import site

tests/node/util/util_aborted_and_transferable.test.ts
  - const ac = new AbortController(); const p = util.aborted(ac.signal, {}); ac.abort(); await p resolves
  - util.aborted(alreadyAbortedSignal, {}) resolves promptly without needing a fresh abort event
  - const tac = util.transferableAbortController(); tac instanceof AbortController === true
  - const ts = util.transferableAbortSignal(ac.signal); ts.aborted === ac.signal.aborted (mirrors state)

tests/node/util/util_worker_defaultoptions_isolation.test.ts (multithread)
  - main thread sets util.inspect.defaultOptions.depth = 5
  - spawn a worker_threads Worker that reads util.inspect.defaultOptions.depth
  - assert the worker observes the ORIGINAL default (e.g. 2), not the main thread's mutation
    (per-thread-region module-state isolation, §5.4 regression guard)

tests/node/util/util_worker_transferable_abort.test.ts (multithread, deferred until
  worker_threads transfer-list design lands, §5.7/§7 — keep as a skipped/pending fixture
  with a tracking comment until then)
  - main thread creates util.transferableAbortController(), transfers .signal to a Worker
    via postMessage(msg, [signal]); worker observes 'abort' when main calls ac.abort()
```

## 7. Open questions / deferrals

- **`util.getCallSites`'s RTS-native mechanism.** No V8-equivalent
  `CallSite`/`Error.prepareStackTrace` machinery exists in RTS (Cranelift-
  based). The plan in §5.1/§5.2 is a native stack-walk (backtrace crate or
  platform unwind APIs) with a synthesized `scriptId`, but this needs
  validation: does RTS emit enough debug info per compiled function
  (source file + line/column) at both JIT and AOT to make this accurate,
  including through inlined/tail-called frames the Cranelift egraph may
  have folded? This is the single heaviest uncertainty in the module.
- **The exact libuv-equivalent negative-error-code table.** §4/§5.1/§5.2
  require RTS to embed its own name↔negative-code↔message table
  matching what real Node/libuv would report for the *same* error on the
  *same* platform, so that `getSystemErrorName(err.errno)` called on an
  error produced by e.g. `node:fs` round-trips correctly. This table
  should be built once and shared with any other `rts-node` module that
  needs `.errno` semantics (`node:fs`, `node:net`, …) rather than
  duplicated — exact ownership location (a shared internal module within
  `rts-node`) needs to be decided when those modules' specs are written.
- **`util.types.isProxy`'s native hook** — does the engine already expose
  an internal "is this value a Proxy" fact usable outside its own
  `get`/`set`/`delete`-trap dispatch machinery? If not, this is a small
  `rts-adapters`/engine-layer addition this spec depends on but does not
  own.
- **`util.types.isModuleNamespaceObject`'s tag** — same open question for
  RTS's ESM module-namespace representation.
- **`util.types.isExternal`'s cross-crate reach into `rts-napi`** — needs
  confirmation of whether `rts-napi` already exposes a usable check, or
  whether one must be added there.
- **`util.types.isCryptoKey`/`isKeyObject`'s coordination with
  `node:crypto`** — depends on that module's `KeyObject`/`CryptoKey`
  class design (not yet written in this doc set); the brand-check
  mechanism (private `Symbol` vs class-identity vs a dedicated internal
  tag) should be decided when `node:crypto`'s own spec/impl exists,
  ideally before this module's item (l) in §5.8 is finalized.
- **`transferableAbortController`/`transferableAbortSignal`'s real
  semantics** are fundamentally blocked on `worker_threads`' own
  `MessagePort`/transfer-list design, which has no spec in this doc set
  yet. Until then these two functions can only be stubbed (§5.7) — this
  is a genuine, explicitly-flagged incompleteness, not a silently-skipped
  detail.
- **`util.promisify`'s multi-value-callback tupling behavior**
  (`callback(null, a, b)` → resolves to `[a, b]`) is asserted from general
  Node knowledge, not confirmed against the fetched v25 doc text verbatim
  — worth a quick live-Node check before finalizing the `.ts` port and the
  corresponding test fixture.
- **`util.parseEnv`'s exact `.env` dialect** (quoting rules, `export`
  prefix tolerance, multiline-value support, comment-recognition edge
  cases such as a `#` inside a quoted value) needs verification against
  Node's actual parser source before the `.ts` port claims full parity —
  flagged in §4 and the test plan.
- **`util.parseArgs`'s exact short-option-grouping/inline-value-attachment
  precedence** (e.g. `-abc` all-boolean grouping vs `-afile` where `-a`
  is `type: 'string'`) needs a precise rule table verified against Node's
  own implementation/tests before the `.ts` port is considered complete.
- **`util.convertProcessSignalToExitCode`'s fallback for an unrecognized
  signal name** (assumed `0` in this spec) is not confirmed against the
  fetched doc text (this function was only added in v25.4.0 and detailed
  edge-case behavior wasn't in the fetched excerpt) — verify before
  shipping.
- **`util.inspect`'s exact ~128 MiB output-cap behavior** (hard truncate
  vs throw vs implementation-defined) was not pinned down precisely by
  the fetched docs — low priority (pathological-input edge case) but
  worth a note-to-self before calling `inspect` "done".
- **`util.getSystemErrorName`'s behavior for a code with no mapping**
  (`undefined` return vs throw) needs verification.
- **`util.setTraceSigInt`'s exact cross-platform scope** — "main thread
  only" is documented, but whether it's a hard no-op or a throw when
  called from a Worker, and whether SIGINT-as-a-concept even applies
  meaningfully on Windows the same way, both need verification against
  live Node behavior before the native hook (§5.2) is finalized.
- **Whether `node:util`'s `.ts` shim should physically share one file with
  `node:sys`'s alias wiring, or whether the alias should be purely a
  module-resolution-table concern with zero `.ts` awareness of being
  aliased** — leaning toward the latter (simpler, matches §5.6's
  data-driven framing) but not yet decided against how other `rts-node`
  module-specifier aliases (if any emerge, e.g. `node:sys`-style
  precedent) end up being implemented elsewhere.
