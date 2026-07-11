# node:module

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:module` |
| Node.js version | 25.x |
| Stability | Mixed — 2 Stable (`builtinModules`, `createRequire`, `isBuiltin`, `syncBuiltinESMExports`, `Module.wrap`, `Module._resolveFilename`/`_load`/`_nodeModulePaths`, `require()`/`require.cache`/`require.resolve`, the compile-cache API as of v25.4.0, the source-map API); 1.2 Release candidate (`registerHooks`, `stripTypeScriptTypes`); 1.1 Active development (`findPackageJSON`); 0 Deprecated (`register()`, superseded by `registerHooks()` as of v25.9.0); 0 Deprecated (`require.extensions`, `module.parent`, since long before v25) |
| Tier | P1 |
| Status | [ ] Not implemented — spec only |
| Import forms | `import moduleNs from 'node:module'`; `import { createRequire, findPackageJSON, isBuiltin, register, registerHooks, stripTypeScriptTypes, syncBuiltinESMExports, enableCompileCache, flushCompileCache, getCompileCacheDir, getSourceMapsSupport, findSourceMap, setSourceMapsSupport, SourceMap, constants, builtinModules } from 'node:module'`; `const moduleNs = require('node:module')` / `require('module')` (bare `'module'` also resolves as a core builtin, matching Node); every CommonJS-format file additionally gets the ambient wrapper parameters `module`, `exports`, `require`, `__filename`, `__dirname` with **no import at all** (see §2 Module Wrapper) |
| Globals exposed | none via `globalThis`; the CJS module wrapper injects the file-scoped (not global) bindings `module`, `exports`, `require`, `__filename`, `__dirname` into every CommonJS-format source file — see §2 "Module Wrapper" |

## 1. Purpose

`node:module` exposes the machinery of Node's own module system: the `Module` class backing every CommonJS file (`module.exports`, `require()`, the module cache, path resolution internals), `createRequire()` for building a `require()` inside ESM code, `isBuiltin()`/`builtinModules` for introspecting the core-module set, the synchronous/asynchronous **customization hooks** (`registerHooks`/`register`) that let user code intercept module resolution and loading for both CJS and ESM, a source-map API (`SourceMap` class, `findSourceMap`), the on-disk module **compile cache** (`enableCompileCache`/`flushCompileCache`), and `stripTypeScriptTypes()` for erasing TypeScript type annotations from a string of source. It is the seam a runtime exposes to let user/tooling code observe and reshape "how does `import`/`require` turn a specifier into running code" — which makes it one of the modules most tightly coupled to a given engine's own module-loading architecture, rather than a portable wrapper over OS/std primitives (contrast with e.g. `node:fs`).

## 2. Exported API surface (COMPLETE)

### Classes

#### `Module` (default export of `node:module`; `require('node:module')` / `require('module')`)

Not an `EventEmitter` subclass — no events. Every `.js`/`.cjs` file executed as CommonJS is wrapped by Node in an instance of this class before running.

```typescript
class Module {
  constructor(id?: string, parent?: Module);

  // instance properties
  exports: any;
  id: string;
  filename: string;
  loaded: boolean;
  /** @deprecated use require.main / module.children instead */
  parent: Module | null | undefined;
  children: Module[];
  path: string;
  paths: string[];
  isPreloading: boolean;

  // instance method
  require(id: string): any;

  // static (constructor-function) properties/methods
  static builtinModules: string[];
  static createRequire(filename: string | URL): NodeRequire;
  static isBuiltin(moduleName: string): boolean;
  static syncBuiltinESMExports(): void;
  static wrap(script: string): string;
  static runMain(): void;

  /** @deprecated legacy internal cache/loader surface, still public */
  static _cache: Record<string, Module>;
  /** @deprecated legacy internal extension-handler table */
  static _extensions: Record<string, (module: Module, filename: string) => void>;
  static _pathCache: Record<string, string>;
  static _resolveFilename(
    request: string,
    parent?: Module,
    isMain?: boolean,
    options?: { paths?: string[] },
  ): string;
  static _load(request: string, parent?: Module, isMain?: boolean): any;
  static _nodeModulePaths(from: string): string[];
}
```

**Module Wrapper.** Before executing a CommonJS file's source, Node wraps it in:

```javascript
(function(exports, require, module, __filename, __dirname) {
  // the file's actual source lives here
});
```

This keeps top-level `var`/`let`/`const` scoped to the file (not real globals) and supplies the five wrapper parameters. `Module.wrap(script)` performs exactly this wrapping and returns the wrapped source string (used by tooling that wants to eval a CJS-shaped body itself). `exports` is a plain pre-bound alias for `module.exports` at wrapper-invocation time — reassigning the local `exports` variable (`exports = {...}`) does **not** change what `require()` returns for that module; only writes to `module.exports` do.

**The injected `require` function.** Every CJS file's wrapper `require` parameter is itself a function with attached properties:

```typescript
interface NodeRequire {
  (id: string): any;
  resolve: {
    (request: string, options?: { paths?: string[] }): string;
    paths(request: string): string[] | null;
  };
  cache: Record<string, Module>;
  /** @deprecated */
  extensions: Record<string, (module: Module, filename: string) => void>;
  main: Module | undefined;
}
```

- `require(id)` — loads a module (builtin, relative file, or `node_modules` package) using the full resolution algorithm (§4) and returns its `exports`. Cached after first load, keyed by resolved absolute filename.
- `require.cache` — object of resolved-filename → `Module` instance; deleting an entry forces the next `require()` of that path to re-execute the file; entries can also be injected to fake/mock a module. Does not apply to native addons (`.node`) — reloading one throws.
- `require.extensions` — **deprecated since v0.10.6**; historically let you register a loader per file extension. RTS should implement it as a no-op-but-present object for compatibility, never as a real extensibility point.
- `require.main` — the `Module` for the process's entry script, or `undefined` if the entry point was not CommonJS (e.g. an ESM entry). Idiomatic "was this file run directly" check: `require.main === module`.
- `require.resolve(request[, options])` — runs the resolution algorithm without loading, returns the absolute path. `options.paths` overrides the default search paths (global folders are still appended). Throws `MODULE_NOT_FOUND`.
- `require.resolve.paths(request)` — returns the array of directories that would be searched, or `null` for a core module.

**`module.require(id)`** — instance-method equivalent of the injected `require()`, scoped to that particular `Module` instance's resolution context; lets code with a `Module` handle (but not lexically inside that file) load as-if-from-there.

**Ambient CJS globals (not `globalThis` properties):** `__filename` (absolute path of the current file, symlinks resolved) and `__dirname` (`path.dirname(__filename)`) are additional wrapper parameters available in every CommonJS file with no import.

#### `Module.SourceMap` (aka `module.SourceMap`, `new (require('node:module')).SourceMap(...)`)

Not an `EventEmitter` subclass — no events.

```typescript
class SourceMap {
  constructor(payload: SourceMapPayload, options?: { lineLengths?: Uint32Array });

  payload: SourceMapPayload;

  findEntry(lineOffset: number, columnOffset: number): SourceMapEntry | {};
  findOrigin(lineNumber: number, columnNumber: number): SourceMapOrigin | {};
}
```

Base class: none. Events: none.

### Top-level functions

| Function | Variant |
|---|---|
| `module.createRequire(filename)` | sync |
| `module.findPackageJSON(specifier[, base])` | sync |
| `module.isBuiltin(moduleName)` | sync |
| `module.register(specifier[, parentURL][, options])` | promise (deprecated v25.9.0) |
| `module.registerHooks(options)` | sync |
| `module.stripTypeScriptTypes(code[, options])` | sync |
| `module.syncBuiltinESMExports()` | sync |
| `module.enableCompileCache([options])` | sync |
| `module.flushCompileCache()` | promise (see note) |
| `module.getCompileCacheDir()` | sync |
| `module.getSourceMapsSupport()` | sync |
| `module.findSourceMap(path)` | sync |
| `module.setSourceMapsSupport(enabled[, options])` | sync |

#### `module.createRequire(filename)`

Builds a `require()` function rooted at `filename`'s directory, for use inside ESM (which has no ambient `require`).

| Name | Type | Optional | Default |
|---|---|---|---|
| `filename` | `string \| URL` | no | — |

Return: `NodeRequire` (a full `require` per §2, with `.cache`/`.resolve`/`.main`/`.extensions`). Throws: `ERR_INVALID_ARG_VALUE` if `filename` is not an absolute path / file `URL`. Variant: sync.

```javascript
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const sibling = require('./sibling-module');
```

#### `module.findPackageJSON(specifier[, base])`

Finds the nearest enclosing `package.json` for a specifier's resolution, without fully resolving/loading it.

| Name | Type | Optional | Default |
|---|---|---|---|
| `specifier` | `string \| URL` | no | — |
| `base` | `string \| URL` | yes | caller's own module URL |

Return: `string | undefined` (absolute path to the `package.json`, or `undefined` if none found). Throws: none documented beyond standard arg-type errors. Variant: sync. **Caveat (Node docs):** must not be used to determine module format, and only consults the built-in default resolver — it does **not** honor custom `resolve` hooks registered via `register`/`registerHooks`.

#### `module.isBuiltin(moduleName)`

| Name | Type | Optional |
|---|---|---|
| `moduleName` | `string` | no |

Return: `boolean` — `true` for both prefixed (`'node:fs'`) and legacy-unprefixed (`'fs'`) core module names, and for the always-prefix-mandatory ones only in their `node:`-prefixed form (`'node:test'` → true, `'test'` → false). Variant: sync.

#### `module.register(specifier[, parentURL][, options])` — **deprecated since v25.9.0**

Registers a module customization-hooks module to run on a **dedicated loader thread**, exchanging messages with the main thread for every `resolve`/`load` call.

| Name | Type | Optional | Default |
|---|---|---|---|
| `specifier` | `string \| URL` | no | — |
| `parentURL` | `string \| URL` | yes | `'data:'` |
| `options` | `{ parentURL?; data?: any; transferList?: object[] }` | yes | `{}` |

Return: `Promise<void>` (resolves once the hooks module's `initialize()` completes, if present). Throws/rejects: propagates errors from loading/initializing the hooks module. Variant: promise. **Requires `--allow-worker`** when the permission model is enabled (the hooks run on a Worker thread). **Deprecated** in favor of `registerHooks()` (sync, in-thread, no message-passing overhead).

#### `module.registerHooks(options)`

Registers **synchronous, in-thread** resolve/load hooks — the current recommended mechanism (Release Candidate as of v25.4.0).

| Name | Type | Optional |
|---|---|---|
| `options.resolve` | `(specifier: string, context: ResolveHookContext, nextResolve: (specifier: string, context?: Partial<ResolveHookContext>) => ResolveFnOutput) => ResolveFnOutput` | yes |
| `options.load` | `(url: string, context: LoadHookContext, nextLoad: (url: string, context?: Partial<LoadHookContext>) => LoadFnOutput) => LoadFnOutput` | yes |

Return: `{ deregister(): void }`. Throws: synchronously, if a registered hook throws or breaks the chain contract (must call `next*` or return `shortCircuit: true`). Variant: sync. Multiple `registerHooks()` calls form a **LIFO chain** — each hook's `next*` calls into the previously-registered hook (or Node's own default resolver/loader at the end of the chain).

```javascript
import { registerHooks } from 'node:module';
const hooks = registerHooks({
  resolve(specifier, context, nextResolve) { return nextResolve(specifier, context); },
  load(url, context, nextLoad) { return nextLoad(url, context); },
});
hooks.deregister();
```

#### `module.stripTypeScriptTypes(code[, options])`

Strips (or transforms) TypeScript type-only syntax from a string, without a full TS type-check.

| Name | Type | Optional | Default |
|---|---|---|---|
| `code` | `string` | no | — |
| `options.mode` | `'strip' \| 'transform'` | yes | `'strip'` |
| `options.sourceMap` | `boolean` | yes | `false` (only honored when `mode: 'transform'`) |
| `options.sourceUrl` | `string` | yes | — |

Return: `string` — in `'strip'` mode, type annotations are blanked out preserving column/line positions (so stack traces still line up); in `'transform'` mode, TS-only constructs (enums, namespaces, parameter properties, etc.) are actually down-leveled to JS, and if `sourceMap: true` a trailing `//# sourceMappingURL=data:application/json;base64,...` comment is appended to the returned string (verify — exact embedding convention not confirmed from fetched docs). Throws: a parse error if `code` is not valid TypeScript. Variant: sync.

```javascript
import { stripTypeScriptTypes } from 'node:module';
stripTypeScriptTypes('const a: number = 1;');
// 'const a         = 1;'
```

#### `module.syncBuiltinESMExports()`

Re-syncs the live-binding properties of Node's builtin **ESM** namespace objects to match whatever a CJS consumer mutated on the corresponding `require('node:x')` object. Does not add/remove which names are exported — only updates values.

Return: `void`. Variant: sync.

#### `module.enableCompileCache([options])`

Enables the on-disk V8 compile cache (parsed-bytecode reuse across process runs) for the current process only (not inherited by already-spawned Workers unless they call it themselves or `NODE_COMPILE_CACHE` is set).

| Name | Type | Optional | Default |
|---|---|---|---|
| `options` | `string \| { directory?: string; portable?: boolean }` | yes | directory from `NODE_COMPILE_CACHE` env var, else `path.join(os.tmpdir(), 'node-compile-cache')` |

Return: `{ status: number; message?: string; directory?: string }` (`status` is one of `module.constants.compileCacheStatus`). Throws: none — failures are reported via `status === FAILED` + `message`. Variant: sync. Stable since v25.4.0 (previously experimental). `portable: true` (added v25.0.0, also renamed `path`→`directory`) makes cache entries independent of the absolute project path, so a moved/copied project directory can still reuse cache entries.

#### `module.flushCompileCache()`

Forces any compile-cache entries accumulated so far to be written to disk immediately (normally flushed at process exit).

Return: per Node's own docs this "completes asynchronously" and fails silently on error (cache misses never break the app); some Node reference material shows it returning a value awaited by callers. RTS should treat the observable contract as: **returns a value that is safe to `await`** (`Promise<void>`) and never throws. Variant: promise (verify exact return-type — conflicting phrasing across Node reference sources; confirm against the actual Node source/TypeScript types before finalizing the RTS `.ts` signature).

#### `module.getCompileCacheDir()`

Return: `string | undefined` — the active compile-cache directory, or `undefined` if the cache was never enabled (including via `NODE_COMPILE_CACHE`). Variant: sync.

#### `module.getSourceMapsSupport()`

Return: `{ enabled: boolean; nodeModules: boolean; generatedCode: boolean }` (verify exact shape — Node's source-map support toggle carries sub-flags for whether `node_modules` sources and eval'd/generated code get source-map handling; treat the boolean-only reading here as a floor, refine against the real Node source before implementation). Variant: sync.

#### `module.findSourceMap(path)`

| Name | Type | Optional |
|---|---|---|
| `path` | `string` | no |

Return: `SourceMap | undefined`. Looks up a previously-loaded module's associated source map (from a `//# sourceMappingURL=` comment or `.map` file Node parsed while loading that module). Variant: sync.

#### `module.setSourceMapsSupport(enabled[, options])`

| Name | Type | Optional | Default |
|---|---|---|---|
| `enabled` | `boolean` | no | — |
| `options` | `{ nodeModules?: boolean; generatedCode?: boolean }` | yes | `{}` |

Return: `void`. Programmatic equivalent of the `--enable-source-maps` CLI flag; affects whether uncaught-exception stack traces are source-mapped. Variant: sync.

### Properties & constants

| Name | Type | Description |
|---|---|---|
| `module.builtinModules` | `string[]` | Every core module name, including `node:`-prefix-only ones (`node:sea`, `node:sqlite`, `node:test`, `node:test/reporters`) alongside legacy unprefixed names (`'fs'`, `'path'`, …). Since v23.5.0 the prefix-only names are included too. |
| `module.constants.compileCacheStatus` | `{ ENABLED: number; ALREADY_ENABLED: number; FAILED: number; DISABLED: number }` | Status codes returned by `enableCompileCache()`. `DISABLED` = `NODE_DISABLE_COMPILE_CACHE=1` was set. |

### Events

None. Neither `Module` nor `Module.SourceMap` is an `EventEmitter` subclass; hook failures and cache errors surface via thrown/rejected errors, not events.

## 3. Types & option objects

```typescript
interface ResolveHookContext {
  conditions: string[];
  importAttributes: Record<string, string>;
  parentURL: string | undefined;
}

interface ResolveFnOutput {
  url: string;
  format?: string | null;
  importAttributes?: Record<string, string>;
  shortCircuit?: boolean; // default false
}

interface LoadHookContext {
  conditions: string[];
  format: string | null | undefined;
  importAttributes: Record<string, string>;
}

type ModuleFormat =
  | 'addon' | 'builtin' | 'commonjs' | 'commonjs-typescript'
  | 'json' | 'module' | 'module-typescript' | 'wasm';

interface LoadFnOutput {
  source: string | ArrayBuffer | NodeJS.TypedArray | null | undefined;
  format: ModuleFormat;
  shortCircuit?: boolean; // default false
}

type NextResolve = (specifier: string, context?: Partial<ResolveHookContext>) => ResolveFnOutput;
type NextLoad = (url: string, context?: Partial<LoadHookContext>) => LoadFnOutput;

interface RegisterHooksOptions {
  resolve?: (specifier: string, context: ResolveHookContext, nextResolve: NextResolve) => ResolveFnOutput;
  load?: (url: string, context: LoadHookContext, nextLoad: NextLoad) => LoadFnOutput;
}

interface RegisterHooksHandle {
  deregister(): void;
}

interface RegisterOptions {
  parentURL?: string | URL;
  data?: any;              // arbitrary structured-cloneable value passed to initialize()
  transferList?: object[]; // transferable objects paired with `data`
}

// asynchronous-hooks module shape (loaded by module.register)
interface AsyncHooksModule {
  initialize?(data: any): Promise<void> | void;
  resolve?(specifier: string, context: ResolveHookContext, nextResolve: (...) => Promise<ResolveFnOutput>): Promise<ResolveFnOutput> | ResolveFnOutput;
  load?(url: string, context: LoadHookContext, nextLoad: (...) => Promise<LoadFnOutput>): Promise<LoadFnOutput> | LoadFnOutput;
}

interface EnableCompileCacheOptions {
  directory?: string;
  portable?: boolean; // added v25.0.0
}

interface EnableCompileCacheResult {
  status: number;       // one of constants.compileCacheStatus
  message?: string;     // present iff status === FAILED
  directory?: string;   // present iff status === ENABLED | ALREADY_ENABLED
}

interface StripTypeScriptTypesOptions {
  mode?: 'strip' | 'transform'; // default 'strip'
  sourceMap?: boolean;          // default false; only honored when mode === 'transform'
  sourceUrl?: string;
}

interface SourceMapPayload {
  version: number;
  sources: string[];
  sourcesContent?: (string | null)[];
  names: string[];
  mappings: string; // base64-VLQ encoded
  file?: string;
  sourceRoot?: string;
}

interface SourceMapEntry {
  generatedLine: number;
  generatedColumn: number;
  originalLine: number;
  originalColumn: number;
  originalSource: string;
  name?: string;
}

interface SourceMapOrigin {
  name: string | undefined;
  fileName: string;
  lineNumber: number;
  columnNumber: number;
}

interface SourceMapsSupportInfo {
  enabled: boolean;
  nodeModules: boolean;
  generatedCode: boolean;
}

// require() shapes — see §2 "The injected require function"
interface NodeRequireResolveOptions { paths?: string[]; }
interface RequireResolve {
  (request: string, options?: NodeRequireResolveOptions): string;
  paths(request: string): string[] | null;
}
interface NodeRequire {
  (id: string): any;
  resolve: RequireResolve;
  cache: Record<string, Module>;
  extensions: Record<string, (module: Module, filename: string) => void>;
  main: Module | undefined;
}

// Module.wrap()'s target signature
type ModuleWrapperFn = (
  exports: any,
  require: NodeRequire,
  module: Module,
  __filename: string,
  __dirname: string,
) => void;
```

## 4. Node semantics & edge cases

- **Two-tier hooks: sync (`registerHooks`) vs async (`register`, deprecated).** `registerHooks()` runs hooks synchronously on the same thread that is loading modules — zero cross-thread messaging, and it can intercept both CJS `require()` and ESM `import` resolution/loading uniformly. `register()` spawns hooks on a **dedicated loader thread** and pays inter-thread-communication overhead for every `resolve`/`load`; as of v25.9.0 it is deprecated in favor of `registerHooks()`. `register()` additionally requires `--allow-worker` under the permission model (it needs to spawn a Worker).
- **LIFO hook chaining.** Multiple hook registrations (sync or async, mixed) form a chain where the **most recently registered** hook runs first and calls `nextResolve`/`nextLoad` to defer to the previous registration (or ultimately Node's own default resolver/loader). A hook must either call the `next*` function or return `{ shortCircuit: true, ... }` — omitting both while also not delegating breaks the chain and throws.
- **`format` values gate what `source` type is legal.** `'addon'`/`'builtin'` require `source: null`; `'json'`/`'module'`/`'module-typescript'` require actual source bytes; `'wasm'` requires an `ArrayBuffer`/`TypedArray`; `'commonjs'`/`'commonjs-typescript'` permit `source` to be `null`/`undefined` (Node's own CJS loader re-reads the file itself in that case) or explicit content.
- **`require(esm)` — loading ESM synchronously via `require()`** (stable since v22.0.0 series, generally available by v25): only works if the target module (and everything it imports) is **fully synchronous** (no top-level `await`) AND is unambiguously ESM (`.mjs` extension, or `.js` with the nearest `package.json`'s `"type": "module"`, or `.js` with no `"type"` field but containing detectable ESM syntax). The return value is the module's **namespace object** (named exports as properties; a `default` export under `.default`; `__esModule: true` is set when there is a default export, for classic interop). A module can special-case what a CJS `require()` sees for it entirely by exporting under the literal string key `'module.exports'` — when present, that single value becomes the whole `require()` result and all other named exports become invisible to CJS consumers. Top-level `await` anywhere in the graph → `require()` throws `ERR_REQUIRE_ASYNC_MODULE` (must use `import()` instead). Gated by `process.features.require_module`; can be disabled process-wide with `--no-require-module` (and traced with `--trace-require-module`).
- **Module resolution algorithm** (`require(X)` from a module at path `Y`) — full LOAD_AS_FILE / LOAD_INDEX / LOAD_AS_DIRECTORY / LOAD_NODE_MODULES / NODE_MODULES_PATHS state machine: core module → return immediately; absolute/relative specifier → `LOAD_AS_FILE` then `LOAD_AS_DIRECTORY`, else throw; `#`-prefixed → package "imports" field; otherwise "package self" resolution, then walk `node_modules` directories from `Y` up to the filesystem root (`NODE_MODULES_PATHS`), each with `LOAD_PACKAGE_EXPORTS` (respects `package.json` `"exports"` conditional map) → `LOAD_AS_FILE` → `LOAD_AS_DIRECTORY`; finally the **global folders** (`$HOME/.node_modules`, `$HOME/.node_libraries`, `$PREFIX/lib/node`) and any `NODE_PATH`-listed directories (colon-delimited on POSIX, semicolon on Windows). Extensionless file lookups try, in order, exact-name / `.js` / `.json` / `.node`; `.cjs`/`.mjs` must be spelled out explicitly.
- **`package.json` fields that steer resolution.** `"type"`: `"module"` makes extensionless/`.js` files in that package scope parse as ESM, `"commonjs"` (or absent) as CJS. `"main"`: legacy single entry-point field consulted by `LOAD_AS_DIRECTORY`. `"exports"`: modern conditional-exports map (`"."`, `"./subpath"`, `{"require": ..., "import": ...}` conditions) that can restrict/redirect what subpaths of a package are importable at all — takes precedence over `"main"` when present.
- **Caching is by resolved absolute filename**, not by the specifier string — two different specifiers resolving to the same file share one `Module`/one execution. Case sensitivity is filesystem-dependent: on case-insensitive filesystems (Windows, default macOS), `require('./foo')` and `require('./FOO')` still cache as **separate** entries even though they hit the same file on disk — a known Node caveat, not a bug to "fix" away in RTS.
- **Circular `require()`.** On a require cycle, the second entry into an in-progress module receives its **current, possibly-incomplete** `module.exports` (whatever was assigned before the `require()` that re-entered the cycle) rather than blocking or erroring — must be preserved exactly (RTS should not "detect and throw" on cycles; that is a deliberate, load-bearing CJS behavior many packages rely on).
- **`module.parent` is deprecated** (v14.6.0/v12.19.0) in favor of `require.main` (entry-point detection) + `module.children` (what a module itself required) — `parent` conflated "first requirer" in a way that doesn't generalize once a module is required from multiple places; RTS should still populate it (best-effort: the first `Module` whose `require()` first loaded this one) for compatibility, while documenting it as legacy.
- **`require.extensions` is deprecated** (v0.10.6) — treat as inert/no-op storage, never as a real hook point (a `.ts`-shim `Proxy`/plain object is enough; nothing in RTS's own loading pipeline should consult it).
- **Mandatory `node:`-prefix modules.** `node:sea`, `node:sqlite`, `node:test`, `node:test/reporters` **must** be required with the `node:` prefix — the unprefixed form (`require('sqlite')`) throws `MODULE_NOT_FOUND`, unlike every other core module which accepts both forms. `isBuiltin('sqlite')` correctly returns `false` while `isBuiltin('node:sqlite')` returns `true`, mirroring this asymmetry.
- **`node:` prefix bypasses `require.cache` fakery for injected/mocked entries** in some documented edge cases (Node's own example shows `require.cache.fs = {...}` intercepting `require('fs')` but not `require('node:fs')`) — RTS should preserve this so cache-injection-based mocking libraries behave identically.
- **Windows vs POSIX path handling.** `NODE_PATH` is colon-delimited on POSIX, **semicolon**-delimited on Windows; `module.paths`/`require.resolve.paths` return platform-native absolute paths; symlink resolution for `__filename`/`module.filename` must match the platform's real-path semantics (junctions on Windows behave differently from POSIX symlinks in some edge cases — verify).
- **Security notes.** `registerHooks`/`register` are a full code-execution interception point — a malicious or buggy hook can rewrite what `import`/`require` return for *any* specifier process-wide, including for other, unrelated packages' internal requires. Node's permission model gates `register()` (not `registerHooks()`, since it does not spawn a Worker) behind `--allow-worker`. `stripTypeScriptTypes()` and `Module.wrap()` operate on developer-supplied strings and do not sandbox the resulting code in any way — they are text transforms, not an execution boundary.
- **Deprecations recap.** `require.extensions` (v0.10.6), `module.parent` (v14.6.0/v12.19.0), `module.register()` (v25.9.0, replaced by `registerHooks()`). No hard-removed APIs from this module as of Node 25.
- **No backpressure concerns** — nothing here is a stream; every call is a bounded, synchronous or single-shot-async operation.

## 5. RTS implementation notes

### 5.1 Native impl mapping

**This module is the one place where RTS's own compiler architecture diverges the most from Node's runtime model, and that divergence must be designed around explicitly rather than papered over.** RTS's own module resolver (`crates/rts-codegen-new/src/front/modules/resolve.rs`) resolves the entire program's import graph **once, at compile/JIT-start time**, into a static set of `Target::File` / `Target::Builtin` / `Target::Unsupported` entries — it is not a per-call, re-entrant runtime resolver the way Node's `Module._resolveFilename`/hook chain is. Concretely:

- **`isBuiltin` / `builtinModules`.** Pure data queries. RTS should maintain a **full canonical Node-25 builtin-name list** (matching real Node, including the `node:`-prefix-mandatory ones) *independently* of `rts_node::NODE_SPECS` (which only lists modules RTS has actually implemented) — so `isBuiltin('some-not-yet-implemented-node-module')` still returns `true` like real Node, and a subsequent `import`/`require` of it fails with a clear "node:X not implemented in RTS yet" diagnostic rather than a generic "unknown module" error. `builtinModules` is the same static list, sorted, JSON-serialized once.
- **`findPackageJSON`.** Pure filesystem walk (`std::fs`, self-contained in `rts-node`, no `rts-std` dependency): walk up from `base`'s directory looking for `package.json`. No dynamic-loader dependency.
- **`createRequire` / `require()` / `Module` / `require.cache` / `Module._cache` / hot-swap-by-cache-mutation.** These assume a **genuinely dynamic runtime loader** — arbitrary new paths can be `require()`d at any point during execution, and mutating `require.cache` changes what a *future* `require()` call returns. RTS's compiled output (JIT or AOT) does not have a bytecode-interpreter loop to re-enter for an unplanned new module the way V8 does; the closest existing RTS primitive is `runtime.eval_file`/`runtime.eval` (dynamic compile-and-run of TS source at runtime — today in `rts-std`, see §5.7) and the **deferred dynamic-import epic (#223)**. Recommended approach: implement `createRequire`/`Module`/`require.cache` as a genuine **native module table** (a `HandleTable`-backed registry keyed by resolved absolute path, storing `{exports, loaded, id, filename, children}`), populated by (a) the statically-known import graph at compile time for anything reachable through ordinary `import`/`require` syntax, and (b) the dynamic-eval seam for any path only discovered at runtime (mirrors real Node's own lazy loading, just gated on the same infra as #223). Static-only `require()` (no dynamic new paths — the overwhelming majority of real-world CJS code) can ship well before the dynamic case.
- **`Module.wrap(script)`.** Pure string operation (wrap in the 5-parameter function-source template) — no dynamic loader needed; useful standalone even before dynamic `require()` exists.
- **`Module._resolveFilename` / `Module._nodeModulePaths` / `require.resolve` / `require.resolve.paths`.** These can be implemented as a *read-only, non-executing* projection of the same resolution algorithm RTS's own compile-time resolver already implements (`front/modules/resolve.rs`'s relative-candidate logic, extended with `node_modules`/`NODE_MODULES_PATHS` walking and `package.json` `"main"`/`"exports"` handling, which the current compile-time resolver does not yet do) — i.e. this module is a natural forcing function to grow RTS's own resolver towards full Node-shaped semantics, benefiting both `node:module`'s API surface and RTS's own bare-specifier/`node_modules` import support (today an honest `Target::Unsupported` bail).
- **`stripTypeScriptTypes`.** RTS already owns a full TypeScript parser (`rts-parser`, SWC-based) as part of its own compilation pipeline. `rts-node` MAY depend on `rts-parser` directly — it is not `rts-shared`/`rts-std`, so the crate-partition ban does not apply — making this the most naturally "free" API in the whole module: parse with SWC, blank out (mode `'strip'`) or down-level (mode `'transform'`) type-only constructs, re-print, and (for `transform` + `sourceMap: true`) reuse `swc_common`'s source-map emission.
- **`SourceMap` class / `findSourceMap` / `getSourceMapsSupport` / `setSourceMapsSupport`.** Self-contained: a small VLQ/base64 source-map decoder (hand-rolled or a vendored crate such as `sourcemap`) plus a binary-search lookup table, entirely inside `rts-node`. `findSourceMap(path)`'s "did a loaded module have an associated source map" bookkeeping ties into whatever module-table entries §5.1's `createRequire`/`Module` work produces (each loaded module records its own source map, if any, at load time) and into RTS's own error/crash-trace formatting (`trace/` namespace, `src/crash.rs`) — which is itself outside `rts-node` today (flag, §5.7).
- **`enableCompileCache` / `flushCompileCache` / `getCompileCacheDir` / `constants.compileCacheStatus`.** Node's V8 compile cache accelerates re-parsing source into V8 bytecode across process runs. RTS has no separate bytecode stage — compilation goes straight to Cranelift IR/native code — but RTS already has an analogous artifact cache (`node_modules/.rts/objs`, `.ometa` metadata, per CLAUDE.md "Artifact layout") that lives in the CLI/build pipeline, not `rts-node`/`rts-std`. Recommended: expose these APIs as a real (if RTS-shaped) knob redirecting to that existing cache's directory setting, rather than inventing a parallel cache with nothing compiled to store in it — see the open question in §7.
- **`register` / `registerHooks`.** Needs the same dynamic-loader seam as `createRequire`/`Module` above, PLUS (for `register` only) genuine `worker_threads` infra RTS does not yet have. Given `registerHooks()` (sync, in-thread) is Node's own recommended path and does not need a Worker, prioritize implementing that first and treat `register()`'s dedicated-loader-thread semantics as a stretch goal gated on `worker_threads` landing.
- **`syncBuiltinESMExports`.** RTS's own builtin-module "ESM view" (however `import fs from 'node:fs'` is represented internally — almost certainly not V8-style live-binding namespace objects) likely makes this a best-effort no-op or a narrow re-sync of whatever mutable shim state RTS does expose for builtins; unlikely to have deep semantics worth chasing given RTS builtins are not V8 ESM namespace objects to begin with.

### 5.2 ABI surface

Symbol convention: `__RTS_FN_NODE_MODULE_<NAME>`. Rich/stateful objects — a `Module` instance, a `SourceMap` instance, a `require()` context (the directory a `createRequire()` call is rooted at), and a `registerHooks()` registration — are opaque `Handle` (u64) values into an `rts-node`-owned slab (or `rts-engine`'s `HandleTable` if confirmed reachable, see §5.7). Everything else (paths, specifiers, JSON-shaped compound results) crosses as `StrPtr`/`Bool`/`I32`.

| Symbol | Args (AbiType) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_MODULE_IS_BUILTIN` | `StrPtr moduleName` | `Bool` | against the full canonical Node-25 name list, not just implemented modules |
| `__RTS_FN_NODE_MODULE_BUILTIN_MODULES` | (none) | `StrPtr` (JSON array) | sync, computed once and cached |
| `__RTS_FN_NODE_MODULE_FIND_PACKAGE_JSON` | `StrPtr specifier, StrPtr base` | `StrPtr` (path, or empty-string sentinel for "not found") | sync, pure fs walk |
| `__RTS_FN_NODE_MODULE_CREATE_REQUIRE` | `StrPtr filename` | `Handle` (require-context) | validates filename is absolute/file-URL; throws `ERR_INVALID_ARG_VALUE` via thread-local error slot otherwise |
| `__RTS_FN_NODE_MODULE_REQUIRE` | `Handle ctx, StrPtr id` | `Handle` (Module instance) | the actual load; see §5.3 re: dynamic-eval dependency for unplanned paths |
| `__RTS_FN_NODE_MODULE_REQUIRE_RESOLVE` | `Handle ctx, StrPtr request, StrPtr pathsJson` | `StrPtr` (absolute path) | throws `MODULE_NOT_FOUND` via error slot |
| `__RTS_FN_NODE_MODULE_REQUIRE_RESOLVE_PATHS` | `Handle ctx, StrPtr request` | `StrPtr` (JSON array, or the literal `"null"` for a core module) | |
| `__RTS_FN_NODE_MODULE_WRAP` | `StrPtr script` | `StrPtr` (wrapped source) | pure string op, no handle needed |
| `__RTS_FN_NODE_MODULE_STRIP_TYPES` | `StrPtr code, Bool transformMode, Bool sourceMap, StrPtr sourceUrl` | `StrPtr` (JSON `{code, map?}`) | backed by `rts-parser`/SWC |
| `__RTS_FN_NODE_MODULE_ENABLE_COMPILE_CACHE` | `StrPtr directory, Bool portable` | `StrPtr` (JSON `{status, message?, directory?}`) | sync |
| `__RTS_FN_NODE_MODULE_FLUSH_COMPILE_CACHE` | (none) | `Handle` (promise) | see §5.3 note on ambiguous Node contract |
| `__RTS_FN_NODE_MODULE_GET_COMPILE_CACHE_DIR` | (none) | `StrPtr` (path, or empty-string sentinel) | sync |
| `__RTS_FN_NODE_MODULE_GET_SOURCE_MAPS_SUPPORT` | (none) | `StrPtr` (JSON `{enabled, nodeModules, generatedCode}`) | sync |
| `__RTS_FN_NODE_MODULE_SET_SOURCE_MAPS_SUPPORT` | `Bool enabled, StrPtr optionsJson` | `Void` | sync |
| `__RTS_FN_NODE_MODULE_FIND_SOURCE_MAP` | `StrPtr path` | `Handle` (SourceMap, or `0` sentinel for "none") | sync |
| `__RTS_FN_NODE_MODULE_SOURCEMAP_NEW` | `StrPtr payloadJson, StrPtr lineLengthsJson` | `Handle` | |
| `__RTS_FN_NODE_MODULE_SOURCEMAP_PAYLOAD` | `Handle sm` | `StrPtr` (JSON payload) | |
| `__RTS_FN_NODE_MODULE_SOURCEMAP_FIND_ENTRY` | `Handle sm, I32 lineOffset, I32 columnOffset` | `StrPtr` (JSON entry, or `"null"`) | |
| `__RTS_FN_NODE_MODULE_SOURCEMAP_FIND_ORIGIN` | `Handle sm, I32 lineNumber, I32 columnNumber` | `StrPtr` (JSON origin, or `"null"`) | |
| `__RTS_FN_NODE_MODULE_SOURCEMAP_FREE` | `Handle sm` | `Void` | |
| `__RTS_FN_NODE_MODULE_REGISTER_HOOKS` | `Handle resolveFn, Handle loadFn` | `Handle` (registration) | `resolveFn`/`loadFn` are `Function`-class handles (`0` = not provided); needs the Function invoke_n bridge to call back into user code mid-resolution |
| `__RTS_FN_NODE_MODULE_REGISTER_HOOKS_DEREGISTER` | `Handle registration` | `Void` | removes it from the LIFO chain |
| `__RTS_FN_NODE_MODULE_REGISTER` | `StrPtr specifier, StrPtr parentUrl, StrPtr dataJson` | `Handle` (promise) | deprecated path; needs `worker_threads`, see §5.7 |
| `__RTS_FN_NODE_MODULE_SYNC_BUILTIN_ESM_EXPORTS` | (none) | `Void` | likely a best-effort no-op, see §5.1 |
| `__RTS_FN_NODE_MODULE_INSTANCE_ID` | `Handle module` | `StrPtr` | |
| `__RTS_FN_NODE_MODULE_INSTANCE_FILENAME` | `Handle module` | `StrPtr` | |
| `__RTS_FN_NODE_MODULE_INSTANCE_PATH` | `Handle module` | `StrPtr` | |
| `__RTS_FN_NODE_MODULE_INSTANCE_PATHS` | `Handle module` | `StrPtr` (JSON array) | |
| `__RTS_FN_NODE_MODULE_INSTANCE_LOADED` | `Handle module` | `Bool` | |
| `__RTS_FN_NODE_MODULE_INSTANCE_IS_PRELOADING` | `Handle module` | `Bool` | |
| `__RTS_FN_NODE_MODULE_INSTANCE_CHILDREN` | `Handle module` | `StrPtr` (JSON array of child module ids) | `.ts` shim resolves ids back to `Module` handles via the module table |
| `__RTS_FN_NODE_MODULE_INSTANCE_EXPORTS_GET` / `_SET` | `Handle module[, PolyValue value]` | `PolyValue` / `Void` | `exports` is a mutable `any`-typed slot, crosses as a tagged `PolyValue` like any other dynamic value, not a raw ABI primitive |

`module.exports` (a genuine dynamic JS value, not a primitive) does not fit the typed-`extern "C"` ABI table the way path strings do — it crosses as a boxed `PolyValue` the same way any other dynamically-typed engine value does at a Registry-resolved call boundary (design doc §10.3), not as a bespoke `node:module`-specific marshalling shape.

### 5.3 Async model

- **Fully synchronous surface (the majority of this module):** `isBuiltin`, `builtinModules`, `findPackageJSON`, `createRequire`, `require()`/`require.resolve`/`require.resolve.paths`, `Module.wrap`, `stripTypeScriptTypes`, `enableCompileCache`, `getCompileCacheDir`, `getSourceMapsSupport`/`setSourceMapsSupport`, `findSourceMap`, the `SourceMap` class, `registerHooks` (the hooks themselves run synchronously in-thread — no promise subsystem involved even though the hook *functions* a user supplies may internally be async-shaped in real Node; RTS's first cut should support the synchronous hook contract only, matching what `registerHooks` actually requires), `syncBuiltinESMExports`. None of these need the tokio runtime.
- **`module.flushCompileCache()`** — modeled as returning a promise-shaped value (see §2/§5.2 ambiguity note) that resolves once any buffered cache writes complete; backed by `promise.create`-equivalent machinery wrapping a blocking `std::fs` write, not genuinely needing the tokio runtime beyond whatever the shared promise-settle mechanism itself requires (§5.7).
- **`module.register(specifier, ...)` (deprecated)** — the only genuinely "async infrastructure-heavy" function in this module: it must load the hooks module **on a dedicated thread** (Node spawns an actual Worker for this), exchange `resolve`/`load` calls as message-passing round-trips, and resolve/reject the returned promise based on the hooks module's `initialize()` outcome. This needs `worker_threads`-equivalent infra (thread + message channel) that does not exist in RTS yet, plus the promise-settle subsystem. Recommended: defer this specific function until `worker_threads` lands, and ship `registerHooks()` (fully synchronous, no Worker) as the complete initial implementation of the hooks feature.
- **Dynamic `require()` of a path not in the statically-resolved import graph** (see §5.1) would need to invoke RTS's own runtime compile-and-run seam (`runtime.eval_file`/`eval`, today `rts-std`) synchronously (Node's `require()` is itself synchronous even though it may internally do I/O) — i.e. a blocking call into the dynamic-compile pipeline, not a promise. This is the single largest technical unknown in the module (tracked in §7).

### 5.4 Multithread / worker interaction

- **`require.cache` / the native module table is per-region, not globally shared.** Per the RTS threading model (`docs/specs/rts-threading-model.md`), each thread/region should own its own module-table state by default (mirroring real Node's `worker_threads`, where each Worker gets its own fresh module cache/registry, not a shared one) — a module `require()`d in one thread does not become visible/cached in another thread's `require.cache` automatically. Only if/when a module's `exports` value is explicitly published across a channel/shared-heap boundary does it need promotion semantics.
- **`registerHooks()`/`register()` registrations are process-thread-scoped, not shared** — matching Node, where `--import`-loaded hook modules or programmatic `registerHooks()` calls apply to the thread they were registered on; a Worker does not automatically inherit the main thread's hooks unless it re-registers them itself (or, for `register()`, unless explicitly propagated via Worker construction options in Node — verify RTS parity need here).
- **`enableCompileCache()` is documented by Node itself as NOT propagating to already-spawned Workers** — each Worker/child process needs its own call or the `NODE_COMPILE_CACHE` environment variable set before spawn. RTS should mirror this: the compile-cache directory setting is per-process-or-thread-region state, not globally shared, consistent with `DnsConfig`-style per-thread state elsewhere in RTS (see `node:dns`'s spec for the analogous pattern).
- **`SourceMap` instances / `Module`/`require`-context handles** are plain data once constructed and safe to read from any thread once fully initialized (consistent with the shard-aware `HandleTable`'s general safety story), but are not meant to be *shared* as live, mutable cross-thread objects — if passed across a `worker_threads` channel, RTS should either reject (structured-clone-style) or reconstruct-from-serialized-form in the target thread, matching how Node itself never makes these objects genuinely shared/live across threads.
- **No `SharedArrayBuffer`/raw shared-memory surface** in this module at all.

### 5.5 Buffer / TypedArray interop

The only byte-oriented surface is the `load` hook's `source: string | ArrayBuffer | TypedArray | null | undefined` (a hook can return raw bytes for `'wasm'`/binary module formats) and `stripTypeScriptTypes`'s`options`/output which are string-only. Since `ArrayBuffer`/`TypedArray` are primordial (engine-owned memory model, per the doctrine), a `load`-hook result crossing the ABI as raw bytes reuses the engine's existing typed-array/ArrayBuffer marshalling directly — no bespoke `node:module` binary-transfer convention needed. Everything else in this module is string/JSON-shaped.

### 5.6 Doctrine placement

`node:module` is **non-primordial** — the engine (`rts-codegen-new`) must never hardcode `"module"` or any of its member names. It resolves exactly like every other `node:` module: `import ... from 'node:module'` maps through `rts_node::ns_prefix_for("node:module")` → `"node_module"` (a pure data lookup against `NODE_SPECS`, no hardcoded arm in codegen), and each call like `node_module.isBuiltin(...)` resolves via `rts_node::node_lookup("node_module.isBuiltin")` to a `NodespaceMember` (`symbol`, `args`, `returns`) — the same mechanism already implemented in `crates/rts-node/src/lib.rs` for `fs`/`path`/`os`/`process`/`util`/`crypto`.

The native-extern / `.ts`-shim split: every symbol in §5.2 is a raw primitive (string in/out, JSON blob, handle lifecycle). All JS-shaped ergonomics — the `Module`/`SourceMap` class wrappers, `require()`'s `.cache`/`.resolve`/`.main`/`.extensions` property assembly, `registerHooks`'s LIFO-chain bookkeeping and the `nextResolve`/`nextLoad` closures, option-object normalization (`enableCompileCache(string)` vs `enableCompileCache({directory, portable})` overload resolution), and the module-wrapper-injected `module`/`exports`/`require`/`__filename`/`__dirname` bindings themselves — live in a `.ts` shim shipped by `rts-node` (e.g. `rts-node/src/module/module.ts` + `require.ts` + `hooks.ts` + `source_map.ts`), plus a compiler-level integration point for actually injecting the wrapper bindings into every CommonJS-format file (that piece is arguably not a `.ts`-shim concern at all but a codegen front-end concern — see §7).

### 5.7 Shared-infra dependencies (FLAG)

- **RTS's own module resolver/loader pipeline.** `crates/rts-codegen-new/src/front/modules/resolve.rs` is today a **compile-time-only**, non-`node_modules`-aware, non-`package.json`-aware resolver living inside the engine crate itself, not `rts-std` — but it is also not inside `rts-node`. `registerHooks()`'s synchronous `resolve`/`load` interception and `require.resolve`/`Module._resolveFilename` all need to observe/extend the **same** resolution algorithm the compiler uses, or Node parity is only skin-deep (a `node:module` hook that doesn't actually affect what the compiler resolves is a lie). This needs a defined seam exposed from the engine/front-end to `rts-node`, independent of the `rts-std` ban — likely the biggest architecture task this module surfaces.
- **Dynamic runtime module loading (`runtime.eval_file`/`eval`).** Needed for genuine `createRequire`/`require()` of paths not in the statically-resolved import graph, and for `register`/`registerHooks`' `load` hook to hand back freshly-fetched source for the compiler to actually compile-and-run mid-execution. Currently lives in `rts-std` (`crates/rts-std/src/runtime/mod.rs`), tied to the JIT-only `runtime_eval_src_jit` fast path and an AOT fallback that shells out to the `rts` binary. Since `rts-node` cannot depend on `rts-std`, this must be hoisted to a shared low crate (or `rts-engine`) before dynamic `require()`/hooks can be real — same blocking dependency as the deferred **dynamic-import epic #223**.
- **Promise/async settle subsystem** (`rts-std`'s `promise` namespace, per `docs/specs/async-promise-function.md`) — needed for `module.register()`'s returned promise and (pending the §2 ambiguity) `module.flushCompileCache()`. Must be reachable from `rts-node` without an `rts-std` dependency.
- **`worker_threads`-equivalent infra** (dedicated thread + message channel) — needed only for `module.register()` (deprecated) to spawn its dedicated loader thread. RTS has no `worker_threads` implementation yet at all; this is a cross-cutting prerequisite shared with the real `node:worker_threads` module itself, not something to build bespoke for `node:module`.
- **RTS's own artifact/compile-cache system** (`node_modules/.rts/objs`, `.ometa` metadata) lives in the CLI/build pipeline, not `rts-std` or `rts-node` — `enableCompileCache`/`flushCompileCache`/`getCompileCacheDir` need either a bridge into that existing system or a deliberate decision to implement an independent, RTS-shaped cache (see §7).
- **RTS's own crash/stack-trace formatting** (`trace/` namespace, `src/crash.rs`) — for `findSourceMap`/`setSourceMapsSupport` to have any real effect (source-mapping actual uncaught-exception stack traces), they need a hook into whichever component formats those traces today; that component is outside `rts-node`.
- **HandleTable.** Module/SourceMap/require-context/hook-registration handles need a `HandleTable`-shaped slab; per the pattern established in other `rts-node` specs, prefer confirming `rts-engine::HandleTable` is importable from `rts-node` without pulling in `rts-std`, over duplicating shard logic independently.
- **`rts-parser` (SWC) dependency** for `stripTypeScriptTypes` is **not** a shared-infra concern (`rts-parser` is not `rts-std`/`rts-shared`), just noting it here as the one place this module reaches for a large existing crate rather than raw `std`.

### 5.8 Implementation phases

1. **(a)** Add `rts-node/src/module/mod.rs` with the `NodespaceSpec` skeleton (`node_module: "module"`, `ns_prefix: "node_module"`); register in `NODE_SPECS`.
2. **(b)** Ship the fully static, no-dynamic-loader-needed subset first: `isBuiltin`/`builtinModules` (against a full canonical Node-25 name list, independent of what's actually implemented), `findPackageJSON` (pure fs walk), `Module.wrap` (pure string op).
3. **(c)** Implement `stripTypeScriptTypes` via `rts-parser`/SWC (strip mode first, transform+sourceMap mode second) — a fully self-contained, high-value win with zero infra blockers.
4. **(d)** Implement the `SourceMap` class + `findSourceMap`/`getSourceMapsSupport`/`setSourceMapsSupport` (self-contained VLQ decoder), wired to whatever module-table entries exist so far (can ship before full dynamic `require()` — static modules already have known source + optional source maps at compile time).
5. **(e)** Resolve the §5.7 blockers in order of leverage: (i) expose a seam from the engine's own resolver (`front/modules/resolve.rs`) that `rts-node` can observe/extend for `require.resolve`/`Module._resolveFilename`/hook interception; (ii) hoist the dynamic-eval seam (`runtime.eval_file`/`eval`) and the promise-settle subsystem so they're reachable without an `rts-std` dependency.
6. **(f)** Implement `createRequire`/`Module`/`require()`/`require.cache`/`module.children`/`module.parent` for the **statically-resolvable** case (everything reachable through ordinary `import`/`require` syntax in the compiled program) — the module table populated at compile time, exposed as real `Module` handles.
7. **(g)** Extend (f) to the dynamic case: `require()` of a path only known at runtime, `require.cache` mutation actually affecting subsequent `require()` calls — gated on (e)'s dynamic-eval hoist.
8. **(h)** Implement `registerHooks()` (synchronous, in-thread) end-to-end: LIFO chain bookkeeping, `nextResolve`/`nextLoad` closures, and genuine interception of the resolver seam from (e).
9. **(i)** Implement `enableCompileCache`/`flushCompileCache`/`getCompileCacheDir`/`constants.compileCacheStatus`, per the design decision in §7 (bridge to RTS's existing artifact cache vs. independent cache).
10. **(j)** Implement `syncBuiltinESMExports` (best-effort, likely thin) and the deprecated `module.register()` once `worker_threads` infra exists (lowest priority — actively deprecated upstream).

## 6. Test plan

```
tests/node/module/module_builtins.test.ts
  - isBuiltin('fs') === true; isBuiltin('node:fs') === true; isBuiltin('wss') === false
  - isBuiltin('sqlite') === false; isBuiltin('node:sqlite') === true (mandatory-prefix asymmetry)
  - isBuiltin('test') === false; isBuiltin('node:test') === true; isBuiltin('node:test/reporters') === true
  - builtinModules is a non-empty string[] containing both 'fs' and 'node:sqlite'-style prefix-only entries

tests/node/module/module_find_package_json.test.ts
  - findPackageJSON('./sibling', import.meta.url) resolves to the nearest package.json path
  - findPackageJSON for a specifier with no enclosing package.json returns undefined
  - findPackageJSON does not consult a registered resolve hook (caveat from Node docs)

tests/node/module/module_wrap.test.ts
  - Module.wrap('console.log(1)') contains the 5 wrapper params in order and the original source verbatim
  - wrapped source, when actually invoked with (exports, require, module, __filename, __dirname), executes correctly

tests/node/module/module_strip_types.test.ts
  - stripTypeScriptTypes('const a: number = 1;') strips the annotation, preserves column alignment
  - stripTypeScriptTypes with mode: 'transform' down-levels a TS enum to a plain object/IIFE
  - stripTypeScriptTypes with mode: 'transform', sourceMap: true produces a result containing a sourceMappingURL comment
  - invalid TypeScript syntax throws a parse error

tests/node/module/module_source_map.test.ts
  - new SourceMap(validV3Payload).payload deep-equals the input payload
  - findEntry(line, col) returns the expected mapped entry for a known simple payload
  - findOrigin(line, col) returns the expected original-source location
  - findSourceMap() on a module compiled with a source map returns a SourceMap instance; on one without, returns undefined
  - setSourceMapsSupport(true) / getSourceMapsSupport() round-trip reflects the flag

tests/node/module/module_create_require_basic.test.ts
  - import { createRequire } from 'node:module'; const require = createRequire(import.meta.url); require('./sibling.js') returns the sibling's exports
  - createRequire(nonAbsolutePath) throws ERR_INVALID_ARG_VALUE
  - require.resolve('./sibling') returns the absolute path without executing the module
  - require.resolve.paths('some-package') returns an array of node_modules candidate dirs; require.resolve.paths('fs') returns null

tests/node/module/module_require_cache.test.ts
  - requiring the same relative path twice returns the identical exports object (cached)
  - deleting require.cache[require.resolve('./sibling')] then requiring again re-executes the module (side effect observed twice)
  - injecting a fake entry into require.cache intercepts a subsequent require() of that specifier
  - require.main === module in the entry file when run directly; undefined characteristics when entry is ESM

tests/node/module/module_circular_require.test.ts
  - classic a.js/b.js circular require reproduces Node's documented interleaved-console-output example exactly, including the "unfinished exports" snapshot semantics

tests/node/module/module_commonjs_esm_interop.test.ts
  - require('./distance.mjs') (fully synchronous ESM) returns the namespace object with named exports
  - a synchronously-requirable ESM module exporting under 'module.exports' collapses require()'s result to that single value
  - requiring an ESM module containing top-level await throws ERR_REQUIRE_ASYNC_MODULE
  - process.features.require_module reflects whether require(esm) is enabled

tests/node/module/module_register_hooks.test.ts
  - registerHooks({ resolve, load }) intercepts a subsequent import()/require() and can redirect it to different source
  - two stacked registerHooks() calls chain LIFO (second-registered hook's nextResolve reaches the first-registered hook, not directly to the default resolver)
  - a hook that neither calls next*() nor returns shortCircuit: true throws
  - hooks.deregister() removes the hook from the chain; subsequent resolutions bypass it

tests/node/module/module_compile_cache.test.ts
  - enableCompileCache() returns { status: ENABLED, directory } on first call
  - enableCompileCache() a second time in the same process returns { status: ALREADY_ENABLED }
  - getCompileCacheDir() reflects the enabled directory; undefined before enabling
  - NODE_DISABLE_COMPILE_CACHE=1 env var makes enableCompileCache() return { status: DISABLED }
  - flushCompileCache() does not throw even with no cache entries accumulated

tests/node/module/module_sync_builtin_esm_exports.test.ts
  - mutating a require('node:querystring')-shaped mutable builtin, then calling syncBuiltinESMExports(), reflects on the corresponding import namespace (best-effort; document RTS's actual scope here given builtins are not V8-style ESM namespace objects)

tests/node/module/module_worker_threads.test.ts (multithread)
  - a module require()'d and cached on the main thread is NOT visible in require.cache of a freshly spawned worker_threads Worker (per-region module table isolation)
  - registerHooks() registered on the main thread does not automatically apply inside a Worker unless re-registered there
  - enableCompileCache() called only on the main thread does not propagate to an already-running Worker (matches Node's own documented non-propagation)
```

## 7. Open questions / deferrals

- **The core architecture mismatch (§5.1/§5.7): RTS resolves imports statically at compile time; Node's module system is a dynamic runtime loader.** Full parity for `Module`/`require.cache`/`registerHooks`/`register`'s live interception depends on RTS gaining a genuine dynamic module-loading capability, which today only exists in nascent form as `runtime.eval_file`/`eval` (and is explicitly tied to the deferred **dynamic-import epic #223**). This is the single biggest open item for the whole module — needs an owner decision on sequencing relative to #223 before phases (e)–(g)/(h) can really start.
- **Compile-cache identity (§5.1/§5.7/§5.8(i)).** Should `enableCompileCache`/`flushCompileCache`/`getCompileCacheDir` be a thin Node-shaped wrapper over RTS's existing `.rts/objs`/`.ometa` artifact cache (recommended — "does something real"), or a fully independent, self-contained cache with no compiled-code counterpart to actually store (a synonym with nothing behind it)? Needs an owner decision.
- **`module.flushCompileCache()`'s exact return contract** — conflicting phrasing in Node's own reference material ("void, asynchronous completion" vs. "returns a value resolved when flushing completes"); confirm against the real Node TypeScript types/source before finalizing the RTS `.ts` signature (flagged inline in §2).
- **`module.getSourceMapsSupport()`'s exact return shape** — the fetched documentation did not give a fully confident shape (`{enabled, nodeModules, generatedCode}` is the best-effort reconstruction here); verify against Node source before implementation.
- **`stripTypeScriptTypes`'s exact source-map embedding convention** in `transform` + `sourceMap: true` mode (inline data-URL comment vs. some other channel) — verify against Node source/tests.
- **Whether `register()`'s deprecated dedicated-loader-thread semantics are worth implementing at all**, given it is actively deprecated upstream in favor of `registerHooks()` and needs the not-yet-existing `worker_threads` infra — candidate for "spec but skip" until/unless real-world RTS users specifically need the async-hooks form (e.g. for hooks modules that must themselves be async).
- **`module.parent` best-effort semantics** — Node deprecated it because "first requirer" doesn't generalize once a module is required from multiple call sites; RTS should decide whether to track a real "first requirer" per module (extra bookkeeping for a legacy, discouraged API) or leave it permanently `undefined`/`null` post-entry-point with a documented gap.
- **Cross-`worker_threads` `Module`/`SourceMap`/hook-registration handle transfer semantics** — Node itself doesn't meaningfully define this (these aren't structured-clone-able live objects); RTS's choice of reject-vs-reconstruct is an implementation call, not a Node-parity requirement, flagged as an open design question for phase (g)/(h).
- **`require.extensions` fidelity** — deliberately implementing it as inert (per §4/§5.1) rather than a real extensibility point; flag here in case some real-world package genuinely depends on registering a custom extension handler (rare, and deprecated since v0.10.6, so treated as out of scope unless proven otherwise).
