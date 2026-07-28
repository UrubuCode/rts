# node:vm

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:vm` |
| Node.js version | 25.x (`https://nodejs.org/docs/latest-v25.x/api/vm.html`) |
| Stability | **2 - Stable** for the core surface (`Script`, `createContext`/`isContext`, `runInContext`/`runInNewContext`/`runInThisContext`, `compileFunction`, `vm.constants`); **1 - Experimental** for `vm.Module`/`vm.SourceTextModule`/`vm.SyntheticModule` (gated behind the `--experimental-vm-modules` CLI flag in real Node); **1 - Experimental** for `vm.measureMemory()`. |
| Tier | P2 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed. The verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import vm from 'node:vm'`; `import { Script, SourceTextModule, SyntheticModule, createContext, isContext, runInContext, runInNewContext, runInThisContext, compileFunction, measureMemory, constants } from 'node:vm'`; CJS: `const vm = require('node:vm')` / `const { Script, ... } = require('node:vm')`. |
| Globals exposed | **None.** Every member is reached through the module namespace object — `node:vm` adds nothing to `globalThis`, unlike `node:perf_hooks`'s `performance` or `node:process`'s `process`. |

## 1. Purpose

`node:vm` lets a program compile and run JavaScript/TypeScript source
dynamically: code can be compiled once (`vm.Script`) and run multiple times,
optionally against a different **global object** than the invoking code's own
(a "contextified" object, prepared by `vm.createContext()`), so that
global-variable reads/writes made by the run code are isolated from — or
selectively bridged to — the caller's globals. It also exposes a low-level,
experimental interface for constructing and linking ECMAScript Module Records
programmatically (`vm.Module` and its two concrete subclasses
`vm.SourceTextModule`/`vm.SyntheticModule`), a convenience compiler for
wrapping a code string in a callable function (`vm.compileFunction`), and a
heap-measurement API (`vm.measureMemory`). **The `node:vm` module is not a
security mechanism — it must never be used to run untrusted code**; RTS must
preserve this exact framing rather than imply any sandboxing guarantee the
module does not actually provide.

In real Node this whole module is a thin binding onto the underlying JS
engine's own isolate/context/script primitives. **RTS has no such engine
underneath it — RTS never embeds, emulates, or links that engine (binding
rule, `docs/node-implementation/architecture.md` §11).** RTS reproduces the
*same API shape* by backing it with its own compiler/runtime: RTS's own
runtime-compile pipeline (the same machinery behind `runtime.eval`/
`eval_file` and `new Function(...)`) stands in for "compile source text", and
a new RTS-native scope/context primitive (not yet fully built — see §5.7)
stands in for "run against a substitute global object". §5 lays out this
mapping in full and is explicit about which parts are a straightforward
wrapper over an already-existing RTS primitive versus genuinely new engine
capability that must be built for this module.

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `class Script`

Compiles code **without** running it; the compiled artifact can then be run
any number of times, optionally against different contexts.

- Extends: nothing (plain class, no `EventEmitter`).
- Events: none.

**Constructor**

##### `new Script(code[, options])`

| Param | Type | Optional | Default |
|---|---|---|---|
| `code` | `string` | no | — |
| `options` | `ScriptOptions \| string` | yes | `{}` |

- Added in: v0.3.1
- Description: Compiles `code` but does not run it. The compiled `Script` is
  not bound to any global object; it is bound fresh **before each run**, just
  for that run. If `options` is a string it is shorthand for
  `{ filename: options }`.
- Throws: a `SyntaxError` (or subclass) if `code` fails to parse; the error's
  attached stack-trace line depends on `displayErrors` semantics carried from
  the *run* call, not construction (construction itself always attaches
  parse-error context).
- Variant: sync.

**Instance properties**

| Property | Type | Description |
|---|---|---|
| `script.cachedDataRejected` | `boolean \| undefined` | Added v5.7.0. Set to `true`/`false` when `cachedData` was supplied to the constructor and V8 accepted/rejected it; `undefined` if no `cachedData` was given. |
| `script.sourceMapURL` | `string \| undefined` | Added v19.1.0, v18.13.0. Set to the URL from a `//# sourceMappingURL=` magic comment in `code`, if present. |

**Instance methods**

| Method | Signature | Description |
|---|---|---|
| `script.createCachedData()` | `(): Buffer` | Added v10.6.0. Produces a V8 code-cache `Buffer` usable as a future `cachedData` option. Callable any number of times, at any point (including before every lazily-compiled inner function has actually been invoked once). |
| `script.runInContext(contextifiedObject[, options])` | `(contextifiedObject: object, options?: RunInContextOptions): any` | Added v0.3.1. Runs the compiled code inside `contextifiedObject` (must already be contextified via `vm.createContext()`), returns the value of the last statement executed. Running code has no access to the caller's local scope. |
| `script.runInNewContext([contextObject[, options]])` | `(contextObject?: object \| typeof constants.DONT_CONTEXTIFY, options?: RunInNewContextOptions): any` | Added v0.3.1. Shortcut for `script.runInContext(vm.createContext(contextObject, options), options)` — creates a context, then runs. |
| `script.runInThisContext([options])` | `(options?: RunInThisContextOptions): any` | Added v0.3.1. Runs inside the context of the current (real) `global` object. No access to local scope, but **does** see the real globals. |

#### `class Module` *(base class, Experimental, requires `--experimental-vm-modules`)*

Low-level counterpart of `vm.Script` for ECMAScript Module Records — mirrors
the spec's Cyclic Module Record. Every `Module` is bound to a context from
creation (unlike `Script`, which is context-free until run). **Not directly
constructible** — always reached via `SourceTextModule`/`SyntheticModule`.

- Extends: nothing.
- Events: none.

**Instance properties**

| Property | Type | Description |
|---|---|---|
| `module.status` | `'unlinked' \| 'linking' \| 'linked' \| 'evaluating' \| 'evaluated' \| 'errored'` | The Cyclic Module Record `[[Status]]` (`'errored'` is RTS/Node's own name for `'evaluated'` + a non-`undefined` `[[EvaluationError]]`). See §4 for the full transition diagram. |
| `module.identifier` | `string` | The identifier set at construction (used in stack traces). |
| `module.error` | `any` | Only readable when `status === 'errored'` — the thrown exception. Reading it in any other status throws. `undefined` itself can be a legitimate thrown value, so this is a real property, not an optional one collapsed onto `undefined`. |
| `module.namespace` | `object` | The module namespace object (`GetModuleNamespace`). Only available **after** `link()`/`linkRequests()`+`instantiate()` complete. |

**Instance methods**

| Method | Signature | Description |
|---|---|---|
| `module.link(linker)` | `(linker: LinkerFunction): Promise<void>` | Links dependencies. Must be called before evaluation, exactly once. `linker(specifier, referencingModule, extra)` must return a `Module`/`Promise<Module>` in the same context, with `status !== 'errored'`; if that returned module is itself `'unlinked'`, `link()` recurses into it with the same `linker`. |
| `module.evaluate([options])` | `(options?: ModuleEvaluateOptions): Promise<undefined>` | Evaluates the module and its dependencies (`Evaluate()` concrete method). See §4 for the sync-vs-async-fulfillment rules (depends on top-level `await` in the graph) and the re-call semantics. Cannot be called while `status === 'evaluating'`. |

#### `class SourceTextModule extends Module` *(Experimental, requires `--experimental-vm-modules`)*

Source Text Module Record — the "normal" ES module case (parsed from actual
module source text), as opposed to `SyntheticModule`'s programmatic exports.

**Constructor**

##### `new SourceTextModule(code[, options])`

| Param | Type | Optional | Default |
|---|---|---|---|
| `code` | `string` | no | — |
| `options` | `SourceTextModuleOptions` | yes | `{}` |

- Added in: v9.6.0
- Description: Parses `code` as module source text. Properties assigned to
  `import.meta` that are themselves objects may leak information outside the
  specified `context` — use `vm.runInContext()` to construct objects that stay
  properly scoped to a context.

**Instance properties**

| Property | Type | Description |
|---|---|---|
| `sourceTextModule.dependencySpecifiers` | `string[]` *(frozen array)* | **Deprecated since v24.4.0/v22.20.0** in favor of `moduleRequests`. The bare specifiers of all requested dependencies. |
| `sourceTextModule.moduleRequests` | `ModuleRequest[]` *(frozen array)* | Added v24.4.0, v22.20.0. Full requested-import descriptors (`specifier`, `attributes`, `phase`) — see §3 and the worked example in §4. |

**Instance methods**

| Method | Signature | Description |
|---|---|---|
| `sourceTextModule.createCachedData()` | `(): Buffer` | Added v13.7.0, v12.17.0. Same semantics as `script.createCachedData()`, callable any number of times before evaluation. |
| `sourceTextModule.linkRequests(modules)` | `(modules: Module[]): undefined` | Added v24.8.0. Batch-links dependencies; `modules[i]` must correspond to `moduleRequests[i]`. Two requests with the same specifier + import attributes must resolve to the **same** module instance or `ERR_MODULE_LINK_MISMATCH` is thrown. Empty array allowed for a module with no dependencies. Call `instantiate()` after linking every module in a cycle. |
| `sourceTextModule.instantiate()` | `(): undefined` | Added v24.8.0. Resolves imported bindings (including re-exports) against the linked requests from `linkRequests()`. Throws synchronously if any binding cannot be resolved. For cyclic graphs, `linkRequests()` must have been called on **every** module in the cycle first. |
| `sourceTextModule.hasTopLevelAwait()` | `(): boolean` | Added v24.9.0. Whether the module itself (not its dependencies) contains a top-level `await`. Corresponds to `[[HasTLA]]`. |
| `sourceTextModule.hasAsyncGraph()` | `(): boolean` | Added v24.9.0. Whether the module *or any dependency* has top-level `await` — walks the full dependency graph (can be slow on large graphs). **Requires the module to already be instantiated**; throws otherwise. |

#### `class SyntheticModule extends Module` *(Experimental, requires `--experimental-vm-modules`)*

Synthetic Module Record — exposes a non-JavaScript/programmatic data source
as a module with a fixed, up-front-declared set of named exports.

**Constructor**

##### `new SyntheticModule(exportNames, evaluateCallback[, options])`

| Param | Type | Optional | Default |
|---|---|---|---|
| `exportNames` | `string[]` | no | — |
| `evaluateCallback` | `(this: SyntheticModule) => void` | no | — |
| `options` | `{ identifier?: string; context?: object }` | yes | `{}` |

- Added in: v13.0.0, v12.16.0
- Description: `exportNames` fixes the module's export binding names up front
  (they cannot be added/removed later). `evaluateCallback` runs synchronously
  during `evaluate()` — its return value is discarded, so an `async`
  `evaluateCallback`'s promise rejection is **lost**, not propagated. Objects
  assigned as exports may leak information outside `context` — same caveat as
  `SourceTextModule`'s `import.meta`.

**Instance methods**

| Method | Signature | Description |
|---|---|---|
| `syntheticModule.setExport(name, value)` | `(name: string, value: any): void` | Sets one export binding slot. **Since v24.8.0, `link()` no longer needs to be called first** (earlier versions required linking before the first `setExport()` call). |

### 2.2 Top-level functions

#### `vm.createContext([contextObject[, options]])`

| Param | Type | Optional | Default |
|---|---|---|---|
| `contextObject` | `object \| typeof vm.constants.DONT_CONTEXTIFY \| undefined` | yes | `undefined` (fresh empty object is contextified for back-compat) |
| `options` | `CreateContextOptions` | yes | `{}` |

- **Returns**: `object` — the (now contextified) object, or the ordinary
  global object if `vm.constants.DONT_CONTEXTIFY` was passed.
- **Throws**: none documented beyond ordinary `TypeError` for a non-object
  `contextObject`.
- **Variant**: sync.
- **Description**: prepares `contextObject` so it can be used with
  `vm.runInContext()`/`script.runInContext()`. Inside scripts run against it,
  the global object is *wrapped* by `contextObject` — every own property of
  `contextObject` acts as a global, in addition to the standard built-ins any
  global object has. Outside VM-run code, `contextObject`'s own properties are
  unaffected by this wrapping (they remain ordinary properties of that plain
  object as seen from the calling code). The `name`/`origin` passed in
  `options` are surfaced through the Inspector API.

#### `vm.isContext(object)`

| Param | Type | Optional |
|---|---|---|
| `object` | `object` | no |

- **Returns**: `boolean` — `true` if `object` was contextified via
  `vm.createContext()`, **or** is the plain global object of a context created
  with `vm.constants.DONT_CONTEXTIFY`.
- **Throws**: none.
- **Variant**: sync.

#### `vm.runInContext(code, contextifiedObject[, options])`

| Param | Type | Optional | Default |
|---|---|---|---|
| `code` | `string` | no | — |
| `contextifiedObject` | `object` | no | — |
| `options` | `VmRunInContextOptions \| string` | yes | `{}` |

- **Returns**: `any` — result of the last statement executed.
- **Throws**: `SyntaxError` on compile failure; `ERR_SCRIPT_EXECUTION_TIMEOUT`/
  `ERR_SCRIPT_EXECUTION_INTERRUPTED` per `timeout`/`breakOnSigint`; any
  uncaught exception the run code itself throws.
- **Variant**: sync.
- **Description**: shortcut for `new vm.Script(code, options).runInContext(contextifiedObject, options)`. `contextifiedObject` must already have been produced by `vm.createContext()`.

#### `vm.runInNewContext(code[, contextObject[, options]])`

| Param | Type | Optional | Default |
|---|---|---|---|
| `code` | `string` | no | — |
| `contextObject` | `object \| typeof vm.constants.DONT_CONTEXTIFY \| undefined` | yes | `undefined` |
| `options` | `VmRunInNewContextOptions \| string` | yes | `{}` |

- **Returns**: `any`.
- **Throws**: same classes as `runInContext`, plus context-creation-time
  failures.
- **Variant**: sync.
- **Description**: does, in order: (1) create a new context; (2) contextify
  `contextObject` if given (or a fresh object if `undefined`, or nothing if
  `DONT_CONTEXTIFY`); (3) compile `code` as a `vm.Script`; (4) run it in that
  context; (5) return the result. Equivalent to
  `(new vm.Script(code, options)).runInContext(vm.createContext(contextObject, options), options)`.

#### `vm.runInThisContext(code[, options])`

| Param | Type | Optional | Default |
|---|---|---|---|
| `code` | `string` | no | — |
| `options` | `VmRunInThisContextOptions \| string` | yes | `{}` |

- **Returns**: `any`.
- **Throws**: `SyntaxError`; timeout/SIGINT errors; run-code exceptions.
- **Variant**: sync.
- **Description**: compiles and runs `code` against the **current real**
  `global` object. No access to the caller's local scope, but full access to
  the real globals (unlike a contextified run).

#### `vm.compileFunction(code[, params[, options]])`

| Param | Type | Optional | Default |
|---|---|---|---|
| `code` | `string` | no | — | (the function **body**) |
| `params` | `string[]` | yes | `[]` |
| `options` | `CompileFunctionOptions` | yes | `{}` |

- **Returns**: `Function` — `code` wrapped as a function taking `params`,
  compiled in `options.parsingContext` if given (else the current context).
- **Throws**: `SyntaxError` on compile failure.
- **Variant**: sync.

#### `vm.measureMemory([options])`

- **Stability**: 1 - Experimental.

| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `MeasureMemoryOptions` | yes | `{}` |

- **Returns**: `Promise<MemoryMeasurement>` — resolves with a V8-specific,
  version-may-change memory report; rejects with `ERR_CONTEXT_NOT_INITIALIZED`
  if the (main) context isn't ready yet.
- **Throws**: (via rejection) `ERR_CONTEXT_NOT_INITIALIZED`.
- **Variant**: promise.
- **Description**: `mode: 'summary'` (default) measures only the main
  context; `mode: 'detailed'` measures every context known to the current
  isolate. `execution: 'default'` (default) waits for the next scheduled GC
  before resolving (may never resolve if the process exits first);
  `execution: 'eager'` triggers a GC immediately to force the measurement.
  Distinct from `v8.getHeapSpaceStatistics()`, which measures heap-space
  occupancy rather than per-context reachability.

### 2.3 Properties & constants

| Name | Type | Description |
|---|---|---|
| `vm.constants` | `object` | Namespace object holding the two well-known symbols below. |
| `vm.constants.USE_MAIN_CONTEXT_DEFAULT_LOADER` | `symbol` *(Added v21.7.0, v20.12.0 — Stability 1.1 Active development)* | Usable as the `importModuleDynamically` option on `Script`/`compileFunction` so `import()` inside the compiled code uses the **main context's own default ESM loader** rather than a custom callback. |
| `vm.constants.DONT_CONTEXTIFY` | `symbol` *(Added v22.8.0, v20.18.0)* | Passed as the `contextObject` argument to `createContext()`/`runInNewContext()`/`script.runInNewContext()` to get an **ordinary** (non-contextified/non-wrapped) global object for the new context — enabling e.g. `Object.freeze(globalThis)` inside that context, which is impossible on a normally-contextified object. |

### 2.4 Events

**None.** No class in this module extends `EventEmitter`; there is no
notification/subscription surface anywhere in `node:vm` (contrast e.g.
`node:perf_hooks`'s `'resourcetimingbufferfull'` or `node:fs`'s watchers).

## 3. Types & option objects

```ts
interface CodeGenerationOptions {
  /** false => eval()/Function()/GeneratorFunction() etc. throw EvalError. Default: true. */
  strings?: boolean;
  /** false => WebAssembly.compile* throws WebAssembly.CompileError. Default: true. */
  wasm?: boolean;
}

/** Extra data passed to a linker / importModuleDynamically callback for one import. */
interface ModuleImportExtra {
  /** The `with { ... }` attributes object (empty object if none given). */
  attributes: Record<string, string>;
  /** Alias of `attributes` (back-compat name from the "import assertions" era). */
  assert: Record<string, string>;
}

type LinkerFunction = (
  specifier: string,
  referencingModule: Module,
  extra: ModuleImportExtra,
) => Module | Promise<Module>;

/** import() dynamic-import resolver, usable on Script/SourceTextModule/compileFunction/
 *  createContext/runInContext/runInNewContext/runInThisContext. */
type ImportModuleDynamicallyCallback = (
  specifier: string,
  referencingScriptOrModule: Script | Module,
  extra: ModuleImportExtra,
) => Module | Promise<Module>;

interface CreateContextOptions {
  /** Human-readable context name, visible via the Inspector API. Default: 'VM Context i'. */
  name?: string;
  /** URL-shaped origin string for display, no trailing slash. Default: ''. */
  origin?: string;
  codeGeneration?: CodeGenerationOptions;
  /** 'afterEvaluate' => microtasks run immediately after script.runInContext()
   *  returns, and are included in the timeout/breakOnSigint scope. */
  microtaskMode?: 'afterEvaluate';
  importModuleDynamically?:
    | ImportModuleDynamicallyCallback
    | typeof USE_MAIN_CONTEXT_DEFAULT_LOADER;
}

interface BaseRunOptions {
  /** Attach the offending source line to a compile-time Error's stack. Default: true. */
  displayErrors?: boolean;
  /** ms before execution is forcibly terminated; must be a strictly positive integer. */
  timeout?: number;
  /** true => SIGINT during execution terminates it and throws; existing
   *  process.on('SIGINT') handlers are suspended for the duration. Default: false. */
  breakOnSigint?: boolean;
}

/** new vm.Script(code, options) */
interface ScriptOptions extends BaseRunOptions {
  /** Stack-trace filename. Default: 'evalmachine.<anonymous>'. */
  filename?: string;
  lineOffset?: number; // default 0
  columnOffset?: number; // default 0
  cachedData?: Buffer | ArrayBufferView;
  /** Deprecated in favor of script.createCachedData(). Default: false. */
  produceCachedData?: boolean;
  importModuleDynamically?:
    | ImportModuleDynamicallyCallback
    | typeof USE_MAIN_CONTEXT_DEFAULT_LOADER;
}

/** script.runInContext(ctx, options) */
interface RunInContextOptions extends BaseRunOptions {}

/** script.runInNewContext(ctxObj, options) */
interface ScriptRunInNewContextOptions extends BaseRunOptions {
  contextName?: string;
  contextOrigin?: string;
  contextCodeGeneration?: CodeGenerationOptions;
  microtaskMode?: 'afterEvaluate';
}

/** vm.runInContext(code, ctx, options) — compiles + runs in one call. */
interface VmRunInContextOptions extends BaseRunOptions {
  filename?: string;
  lineOffset?: number;
  columnOffset?: number;
  cachedData?: Buffer | ArrayBufferView;
  importModuleDynamically?:
    | ImportModuleDynamicallyCallback
    | typeof USE_MAIN_CONTEXT_DEFAULT_LOADER;
}

/** vm.runInNewContext(code, ctxObj, options) */
interface VmRunInNewContextOptions extends VmRunInContextOptions {
  contextName?: string;
  contextOrigin?: string;
  contextCodeGeneration?: CodeGenerationOptions;
  microtaskMode?: 'afterEvaluate';
}

/** vm.runInThisContext(code, options) */
interface VmRunInThisContextOptions extends BaseRunOptions {
  filename?: string;
  lineOffset?: number;
  columnOffset?: number;
  cachedData?: Buffer | ArrayBufferView;
  importModuleDynamically?:
    | ImportModuleDynamicallyCallback
    | typeof USE_MAIN_CONTEXT_DEFAULT_LOADER;
}

interface CompileFunctionOptions {
  filename?: string; // default ''
  lineOffset?: number; // default 0
  columnOffset?: number; // default 0
  cachedData?: Buffer | ArrayBufferView;
  /** Default: false. */
  produceCachedData?: boolean;
  /** Contextified object to compile the function in; default: current context. */
  parsingContext?: object;
  /** Extra scope-wrapping objects applied while compiling. Default: []. */
  contextExtensions?: object[];
  importModuleDynamically?:
    | ImportModuleDynamicallyCallback
    | typeof USE_MAIN_CONTEXT_DEFAULT_LOADER;
}

interface SourceTextModuleOptions {
  /** Stack-trace identifier. Default: 'vm:module(i)'. */
  identifier?: string;
  cachedData?: Buffer | ArrayBufferView;
  /** A contextified object from vm.createContext(); default: current context. */
  context?: object;
  lineOffset?: number; // default 0
  columnOffset?: number; // default 0
  initializeImportMeta?: (meta: ImportMeta, module: SourceTextModule) => void;
  importModuleDynamically?: ImportModuleDynamicallyCallback;
}

interface SyntheticModuleOptions {
  identifier?: string;
  context?: object;
}

interface ModuleEvaluateOptions {
  timeout?: number;
  breakOnSigint?: boolean;
}

/** sourceTextModule.moduleRequests element shape (added v24.4.0/v22.20.0). */
interface ModuleRequest {
  specifier: string;
  /** The `with { ... }` attribute object; {} if none. */
  attributes: Record<string, string>;
  /** 'evaluation' for a normal import; 'source' for `import source X from '...'`. */
  phase: 'evaluation' | 'source';
}

interface MeasureMemoryOptions {
  /** 'summary': only the main context. 'detailed': every context in the isolate. Default: 'summary'. */
  mode?: 'summary' | 'detailed';
  /** 'default': wait for the next scheduled GC. 'eager': trigger GC immediately. Default: 'default'. */
  execution?: 'default' | 'eager';
}

/** Per-context (or aggregate) memory figures — V8-internal shape, may change
 *  across V8/Node versions; RTS reproduces the currently-documented shape. */
interface ContextMemoryUsage {
  jsMemoryEstimate: number;
  /** [lowerBound, upperBound] byte range V8 is confident the true value lies within. */
  jsMemoryRange: [number, number];
}

/** vm.measureMemory()'s resolved value. `current`/`other` only present in 'detailed' mode. */
interface MemoryMeasurement {
  total: ContextMemoryUsage;
  current?: ContextMemoryUsage;
  other?: ContextMemoryUsage[];
}
```

## 4. Node semantics & edge cases

- **Security.** "The `node:vm` module is not a security mechanism. Do not use
  it to run untrusted code." This is a direct quote from Node's own docs and
  RTS must not contradict it anywhere in user-facing documentation — a
  contextified object is an isolation *convenience* for trusted code
  (plugin-style API surfaces, template engines, config DSLs), never a
  sandbox against hostile input.
- **What "contextify" means.** `vm.createContext(obj)` prepares `obj` so code
  run against it sees `obj`'s own properties as if they were global variables,
  *in addition to* the standard built-ins any global object has. Writes to
  globals made by the run code are reflected back onto `obj`; reads of `obj`'s
  properties from **outside** VM-run code are unaffected — they're just
  ordinary property reads on a plain object. `vm.constants.DONT_CONTEXTIFY`
  (v22.8.0/v20.18.0) opts out of this wrapping entirely and hands back a truly
  ordinary global object for the new context — the documented use case is
  `Object.freeze(globalThis)`, which cannot be done on a normally-contextified
  object (the wrapping machinery relies on the object staying mutable).
- **`vm.isContext()`** returns `true` both for objects contextified the normal
  way and for the plain global object of a `DONT_CONTEXTIFY` context — "is a
  context" and "is contextified" are not synonyms.
- **Local scope is never visible.** Every run entry point (`runInContext`,
  `runInNewContext`, `runInThisContext`, and their `Script`-method
  equivalents) explicitly documents "does not have access to local scope" —
  code run through `vm` sees only the target global object (plus, for
  `runInThisContext`, the caller's *real* globals) — never the calling
  function's local variables/closures.
- **`microtaskMode: 'afterEvaluate'`.** Without this, microtasks scheduled by
  `Promise`s/`async function`s inside the run code are **not** drained as
  part of the `runIn*` call — they run later, on the outer/normal microtask
  queue. With it, they're drained immediately after the script finishes
  running, and are counted *inside* the `timeout`/`breakOnSigint` window.
- **`timeout`/`breakOnSigint` cost.** Documented explicitly: "Using the
  `timeout` or `breakOnSigint` options will result in new event loops and
  corresponding threads being started, which have a non-zero performance
  overhead." Not free — do not default them on.
- **`cachedData`.** A `Buffer`/`TypedArray`/`DataView` holding V8's serialized
  bytecode cache for the *exact same* source text (and, for
  `SourceTextModule`, the exact same module). `script.cachedDataRejected`
  records whether V8 accepted it; passing stale/mismatched data does not
  throw — it just gets rejected and recompiled from source. `produceCachedData`
  (constructor option) is deprecated in favor of the always-available
  `createCachedData()` method, callable any number of times.
- **`Module.status` transition diagram**: `'unlinked'` (initial, before
  `link()`/`linkRequests()`) → `'linking'` (linker `Promise`s pending) →
  `'linked'` (fully linked, deps linked, not yet evaluated) → `'evaluating'`
  (mid `evaluate()`, on itself or a parent) → `'evaluated'` (success) or
  `'errored'` (evaluation threw — Node's own name for spec `'evaluated'` +
  non-`undefined` `[[EvaluationError]]`).
- **`module.evaluate()` fulfillment timing** depends on top-level `await`:
  with none anywhere in the graph, the returned promise settles
  **synchronously** (both success and failure); with top-level `await`
  anywhere in the graph, it settles **asynchronously** either way. For a
  `SyntheticModule`, `evaluate()` **always** settles synchronously — the
  `evaluateCallback`'s own asynchronous behavior (if it happens to be an
  `async function`) is invisible: its return value is discarded and any
  rejection from it is silently lost, not propagated.
- **Re-calling `evaluate()`** after a prior evaluation: success → no-op,
  resolves to `undefined` again; failure → **re-rejects with the same original
  exception** rather than re-running. Calling it **while** `status ===
  'evaluating'` is an error.
- **`linkRequests()`/`instantiate()` (v24.8.0) vs the older `link()`.**
  `linkRequests(modules)` is a synchronous batch-link keyed by
  `moduleRequests` order; **cyclic** dependency graphs require calling
  `linkRequests()` on **every** module in the cycle before calling
  `instantiate()` on any of them. Two requests sharing the same specifier +
  attributes must resolve to the identical module instance —
  `ERR_MODULE_LINK_MISMATCH` otherwise.
- **`dependencySpecifiers` deprecation (v24.4.0/v22.20.0)**: superseded by
  `moduleRequests`, which additionally carries import attributes and phase.
  Worked example (from Node's own docs) for:
  ```ts
  import foo from 'foo';
  import fooAlias from 'foo';
  import bar from './bar.js';
  import withAttrs from '../with-attrs.ts' with { arbitraryAttr: 'attr-val' };
  import source Module from 'wasm-mod.wasm';
  ```
  yields `moduleRequests` = `[{specifier:'foo',attributes:{},phase:'evaluation'}, {specifier:'foo',attributes:{},phase:'evaluation'}, {specifier:'./bar.js',attributes:{},phase:'evaluation'}, {specifier:'../with-attrs.ts',attributes:{arbitraryAttr:'attr-val'},phase:'evaluation'}, {specifier:'wasm-mod.wasm',attributes:{},phase:'source'}]` — note the duplicate `'foo'` entry (once per import statement, not deduplicated by specifier) and `phase: 'source'` for a WebAssembly module-source import.
- **`setExport()` no longer requires `link()` first (v24.8.0)** — earlier
  versions required linking before the first call; RTS should implement the
  current (unrestricted) behavior only, since it targets Node 25.
- **`sourceTextModule.hasAsyncGraph()`** requires the module to already be
  instantiated; calling it earlier throws. It can be slow on large graphs
  (full dependency walk) — document this as an inherent cost, not a bug.
- **`vm.measureMemory()` result shape is V8-internal and may change** between
  V8/Node versions — Node's own docs give this exact example for `'summary'`
  mode: `{ total: { jsMemoryEstimate: 2574732, jsMemoryRange: [0, 3936732] } }`,
  and for `'detailed'` mode additionally `current` (the calling context) and
  `other` (an array, one entry per other known context). `execution: 'default'`
  can in principle never resolve if the process exits before its next
  scheduled GC — `'eager'` avoids that by forcing one.
- **Platform differences.** No `vm`-specific Windows/POSIX divergence is
  documented; `breakOnSigint`'s SIGINT handling rides on the same
  process-wide signal-translation Node already needs for `process.on('SIGINT')`
  (on Windows this is libuv's Ctrl+C-to-SIGINT translation, not a native
  POSIX signal) — RTS should reuse whatever its own `process` module already
  does for `SIGINT`, not invent a second path.
- **Error codes** (from Node's error-code reference):

  | Code | Meaning |
  |---|---|
  | `ERR_VM_DYNAMIC_IMPORT_CALLBACK_MISSING` | A dynamic import callback was not specified — thrown when `import()` executes inside VM-compiled code that has no `importModuleDynamically` and is not using `USE_MAIN_CONTEXT_DEFAULT_LOADER`. |
  | `ERR_VM_DYNAMIC_IMPORT_CALLBACK_MISSING_FLAG` | A dynamic import callback was invoked without `--experimental-vm-modules`. |
  | `ERR_VM_MODULE_ALREADY_LINKED` | The module has already been linked (`linkingStatus` is `'linked'`), is currently linking, or a previous link attempt failed. |
  | `ERR_VM_MODULE_CACHED_DATA_REJECTED` | The `cachedData` option passed to a module constructor is invalid. |
  | `ERR_VM_MODULE_CANNOT_CREATE_CACHED_DATA` | Cached data cannot be created for a module that has already been evaluated. |
  | `ERR_VM_MODULE_DIFFERENT_CONTEXT` | The module returned from the linker function belongs to a different context than the parent module. |
  | `ERR_VM_MODULE_LINK_FAILURE` | The module could not be linked, due to a failure. |
  | `ERR_VM_MODULE_NOT_MODULE` | The fulfilled value of a linking promise is not a `vm.Module` object. |
  | `ERR_VM_MODULE_STATUS` | The current module's status does not allow the requested operation. |
  | `ERR_SCRIPT_EXECUTION_TIMEOUT` | Script execution timed out, possibly due to bugs in the script being executed. |
  | `ERR_SCRIPT_EXECUTION_INTERRUPTED` | Script execution was interrupted by `SIGINT` (e.g. Ctrl+C). |
  | `ERR_CONTEXT_NOT_INITIALIZED` | The vm context passed into the API is not yet initialized (thrown/rejected by `vm.measureMemory()`). |
  | `ERR_MODULE_LINK_MISMATCH` | A module cannot be linked because the same module request in it is not resolved to the same module instance across calls. |

- **`--experimental-vm-modules`.** `vm.Module`/`SourceTextModule`/
  `SyntheticModule` require this CLI flag in real Node; RTS's own equivalent
  gating mechanism (a flag, or simply "always on" since RTS controls its own
  release cadence) is an implementation decision — see §5.8/§7.
- **Deprecations to preserve**: `Script`'s `produceCachedData` constructor
  option (use `createCachedData()`); `sourceTextModule.dependencySpecifiers`
  (use `moduleRequests`).

## 5. RTS implementation notes

### 5.1 Native impl mapping

Per the project's binding no-V8 rule (`docs/node-implementation/architecture.md`
§11): **RTS never embeds, emulates, or links V8 (or any other third-party JS
engine).** `node:vm`'s entire surface is a binding onto V8's own isolate and
context primitives in real Node — RTS backs the *same API shape* with its own
Cranelift JIT + its own runtime-compile pipeline. This module sits at the
**opposite end of the coupling spectrum** from `fs`/`os`/`path` (pure OS
wrapping, zero engine coupling): almost all of its native work *is* engine
work, and rts-node itself contributes very little standalone Rust.

- **The compile-and-run primitive already exists, narrower than `vm` needs.**
  RTS already has a "compile TS/JS source at runtime and execute it" seam:
  the `runtime.eval`/`runtime.eval_file` symbols (today `rts-std`, see
  `crates/rts-std/src/runtime/mod.rs`) and the `new Function(...)` dynamic
  compile path (the engine's own function-at-runtime lowering, referenced in
  `docs/specs/async-promise-function.md` and `.claude/rules/03-features.md`,
  living inside `rts-codegen-new`, since only the engine itself owns the
  Cranelift lowering pipeline needed to turn source text into callable code).
  `vm.compileFunction()` in the **no-`parsingContext`** case is essentially
  this exact primitive with a Node-shaped signature — a thin wrapper, not new
  engine work.
- **What genuinely does not exist yet: a "compile bound to an alternate
  scope" primitive.** `vm.createContext()`/`runInContext()`/the `context`
  option on `SourceTextModule`/`SyntheticModule` all need code whose free
  (global) identifier lookups resolve against a *substitute* global-binding
  table instead of the process's real globals. RTS's engine has no such
  scope-indirection mechanism today (its dynamic-compile primitives bind
  against the one real global scope, or a subprocess's own single
  entry-point). Building this is genuinely new engine capability, not
  something achievable by rts-node's own Rust code, since only the engine
  (which owns Cranelift lowering + identifier resolution) can decide what a
  free identifier resolves to. Two shapes were considered:
  1. **(Recommended direction) A scope/region handle threaded through the
     existing compile-and-run entry point.** The engine's compile primitive
     grows an optional "bind free globals against this handle instead of the
     real global table" parameter — conceptually a lightweight analogue of
     the per-thread/per-region private-globals mechanism the RTS threading
     model already establishes (`docs/specs/rts-threading-model.md`,
     `threadLocal`) and that GCELLS-thread-locality already demonstrates in
     practice (see `MEMORY.md`'s "GCELLS thread-local" gotcha from the timer
     work) — except keyed by an explicit context handle rather than by OS
     thread.
  2. **(Rejected for now, but simplest partial) Property-bag merge, not real
     scope substitution.** Treat "the context's globals" as a plain JS object
     the compiled code's *unresolved* identifiers fall back to reading/writing
     via a Proxy-like indirection at the `.ts`/registry level, while the code
     still technically compiles against the real global scope underneath.
     This is closer to how a naive `with(contextObj) { ... }` might behave
     than to V8's actual per-isolate global object substitution, and would
     visibly diverge from Node semantics (e.g. declaring `function`/`var` at
     the top level of run code would leak into the **real** global scope, not
     just the context object) — acceptable only as a clearly-flagged interim
     step (§5.8 phase d), never as the final implementation.
- **`vm.createContext()`/`isContext()` bookkeeping.** Whichever scope
  mechanism lands, "is this object a context" needs a fast, native answer —
  a small side-table (a `HashSet`/tagged `HandleTable` entry) mapping
  contextified objects to their scope handle, checked by `isContext()` and
  consulted by every `runIn*` entry point to validate its `contextifiedObject`
  argument.
- **`vm.constants.DONT_CONTEXTIFY`.** Simpler than the general case — it just
  means "run against the engine's ordinary (real) global scope, with no
  wrapping/substitution at all" — effectively identical machinery to
  `runInThisContext()`, just packaged through `createContext()`'s calling
  convention. `Object.freeze(globalThis)` afterward reuses whatever
  `Object.freeze`/`isFrozen` support the engine already has for ordinary
  objects (already implemented per project history — see `MEMORY.md`'s GC
  overhaul notes).
- **`vm.Script`/`vm.compileFunction` compile-time options** (`filename`,
  `lineOffset`, `columnOffset`) map onto whatever source-position metadata the
  engine's parser (`rts-parser`) and error/stack-trace machinery
  (`trace/` namespace, `src/crash.rs`) already track and can be told to
  offset — no new capability, just plumbing existing parser diagnostics
  through a Node-shaped option object.
- **`cachedData`/`createCachedData()`.** V8's bytecode cache has no RTS
  analogue — RTS has no bytecode VM, only Cranelift-compiled native code.
  The closest genuine equivalent is RTS's own existing artifact cache
  (`node_modules/.rts/objs/**/*.ometa`, per the Artifact layout section of
  `CLAUDE.md`) or a serialized parsed-AST/HIR blob. Until a real format is
  designed (tie to the same open compile-cache-identity question
  `docs/node-implementation/module.md` §5.1/§5.7/§7 raises for
  `enableCompileCache`), `createCachedData()` should return an honest
  placeholder (an empty `Buffer`, clearly documented as non-functional) and
  `cachedDataRejected` should always read `true` whenever `cachedData` was
  supplied — an explicit, flagged interim behavior, never a silent
  "pretends to work" stub. See §5.8(g)/§7.
- **`vm.measureMemory()`.** Maps onto `rts-engine`'s own heap/`HandleTable`
  statistics (live-handle counts, mark+sweep collector byte accounting) —
  the same source `node:v8`'s `getHeapStatistics()` uses per
  `architecture.md` §11's "Heap fields elsewhere" note. `'detailed'` mode's
  per-context breakdown is only meaningful once contexts are real, separately
  trackable scope handles (§5.8 phase d/h ordering).
- **`vm.Module`/`SourceTextModule`/`SyntheticModule`.** `SourceTextModule`
  needs the engine to parse+compile module-shaped source (import/export
  syntax) **without** immediately resolving its imports against RTS's own
  static, compile-time module graph (`front/modules/resolve.rs`/
  `flatten.rs`) — it needs a genuinely **dynamic** module record the user
  links/instantiates by hand. This is the same "RTS resolves imports
  statically; Node's (and V8's) module system is a dynamic runtime construct"
  architecture mismatch `docs/node-implementation/module.md` flags for the
  `#223` dynamic-import epic — `node:vm`'s `Module` family is arguably the
  **purest expression** of that gap, since its entire contract is "give me an
  unlinked module record I control the linking of." `SyntheticModule` is
  comparatively easy (no parsing at all — just a fixed set of named export
  slots a callback fills), and could reasonably ship well before
  `SourceTextModule`.

### 5.2 ABI surface

Symbols `__RTS_FN_NODE_VM_<NAME>`, registered under nodespace `vm`
(`ns_prefix = "node_vm"`) in `rts-node`'s own `NodespaceSpec`/`NODE_SPECS`
table (`crates/rts-node/src/lib.rs`, same pattern as `fs`/`path`/`os`/
`process`/`util`/`crypto`).

**Opaque-`any` convention (read this before the table).** Several members
carry genuinely arbitrary JS values across the boundary: a run's return value,
a contextified object's global-property values, a `SyntheticModule` export
value, `import.meta`/`ModuleRequest.attributes` objects. Since `rts-node`
cannot depend on `rts-runtime's adapters module` (the crate owning the `PolyValue` NaN-box
Rust type), these cross as an **opaque, uninterpreted `I64`** — the raw
NaN-boxed bit pattern — which `rts-node` only ever stores and returns
unchanged, never decodes. This mirrors a pattern already established
elsewhere in the engine (`rts-engine`'s `collections`/`buffer` namespaces
already treat a generic `i64` payload word as opaque, caller-interpreted
data); it lets `node:vm` support arbitrary "any"-typed values with **zero**
new Cargo dependency, because the actual box/unbox happens as pure Cranelift
IR the *codegen* emits at each call site (`docs/specs/rts-codegen-new-design.md`
§9.3), not as Rust code `rts-node` would need to write.

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_VM_CONTEXT_CREATE` | `StrPtr` (name), `StrPtr` (origin), `Bool` (dont_contextify) | `Handle` | Allocates a scope/context record (§5.1 item 2). Opaque `Handle`, not yet backed by real free-identifier substitution until the engine capability lands — see §5.7. |
| `__RTS_FN_NODE_VM_CONTEXT_IS` | `Handle` | `Bool` | Side-table membership check for `vm.isContext()`. |
| `__RTS_FN_NODE_VM_CONTEXT_GET_GLOBAL` | `Handle` (context) | `I64` *(opaque tagged value)* | Reads the context's backing "globals" object handle (a plain `Object`, itself primordial) for the `.ts` layer to read/write properties on directly rather than needing per-property externs. |
| `__RTS_FN_NODE_VM_SCRIPT_COMPILE` | `StrPtr` (code), `StrPtr` (filename), `I32` (line_offset), `I32` (column_offset) | `Handle` (script) | Parses/compiles `code` without running it. Throws-equivalent: a compile failure sets a thread-local error slot the `.ts` layer checks (existing engine error-slot convention, `docs/specs/async-promise-function.md`). |
| `__RTS_FN_NODE_VM_SCRIPT_RUN` | `Handle` (script), `Handle` (context, `0` = real global scope), `I64` (timeout_ms, `0` = none), `Bool` (break_on_sigint) | `I64` *(opaque tagged value)* | The one execution primitive backing `runInContext`/`runInNewContext`/`runInThisContext` — the `.ts` shim supplies `context = 0` for `runInThisContext`. |
| `__RTS_FN_NODE_VM_SCRIPT_CREATE_CACHED_DATA` | `Handle` (script) | `Handle` (Buffer) | Interim: empty-`Buffer` placeholder (§5.1) until a real cache format is designed. |
| `__RTS_FN_NODE_VM_SCRIPT_SOURCE_MAP_URL` | `Handle` (script) | `StrPtr` (empty if none) | Best-effort scan for a `//# sourceMappingURL=` magic comment during compile. |
| `__RTS_FN_NODE_VM_SCRIPT_FREE` | `Handle` (script) | `Void` | Releases the compiled-script record. |
| `__RTS_FN_NODE_VM_COMPILE_FUNCTION` | `StrPtr` (code), `Handle` (params, a Vec-of-string Buffer-like handle), `StrPtr` (filename), `I32` (line_offset), `I32` (column_offset), `Handle` (parsing_context, `0` = current) | `I64` *(opaque tagged Function value)* | Thin wrapper over the engine's existing dynamic-function-compile primitive (§5.1) — the one member of this table that is close to a pure pass-through. |
| `__RTS_FN_NODE_VM_MODULE_NEW_SOURCE_TEXT` | `StrPtr` (code), `StrPtr` (identifier), `Handle` (context, `0` = current), `I32` (line_offset), `I32` (column_offset) | `Handle` (module) | Parses module-shaped source into an **unlinked** module record (§5.1's biggest new-capability item). |
| `__RTS_FN_NODE_VM_MODULE_NEW_SYNTHETIC` | `Handle` (export_names, string-array handle), `Handle` (context) | `Handle` (module) | Allocates a `SyntheticModule` record with fixed export slots, all initially unset. |
| `__RTS_FN_NODE_VM_MODULE_STATUS` | `Handle` (module) | `I32` (status enum) | `.ts` maps the `I32` to the 6 string status values. |
| `__RTS_FN_NODE_VM_MODULE_ERROR` | `Handle` (module) | `I64` *(opaque tagged value)* | Only meaningful when status is `'errored'`; `.ts` guards the read per §4. |
| `__RTS_FN_NODE_VM_MODULE_NAMESPACE` | `Handle` (module) | `I64` *(opaque tagged Object value)* | Only meaningful post-link/instantiate. |
| `__RTS_FN_NODE_VM_MODULE_REQUESTS_COUNT` / `__RTS_FN_NODE_VM_MODULE_REQUEST_AT` | `Handle` / `Handle`, `I32` (index) | `I32` / a packed `(StrPtr specifier, I64 attributes_obj, I32 phase)` | Backs `moduleRequests`; `.ts` assembles the frozen array. |
| `__RTS_FN_NODE_VM_MODULE_LINK_REQUESTS` | `Handle` (module), `Handle` (modules, a `Handle`-array) | `Bool` (success) | Backs `linkRequests()`; a mismatch (same specifier+attributes → different instance across two requests) sets the error slot for `ERR_MODULE_LINK_MISMATCH`. |
| `__RTS_FN_NODE_VM_MODULE_INSTANTIATE` | `Handle` (module) | `Bool` (success) | Resolves bindings; failure sets the error slot. |
| `__RTS_FN_NODE_VM_MODULE_EVALUATE` | `Handle` (module), `I64` (timeout_ms), `Bool` (break_on_sigint) | `Handle` (Promise) | Backs both `SourceTextModule` and `SyntheticModule` evaluation — the sync-vs-async settle-timing rule (§4) is engine/promise-subsystem logic, not something this one symbol needs to encode differently. |
| `__RTS_FN_NODE_VM_MODULE_CREATE_CACHED_DATA` | `Handle` (module) | `Handle` (Buffer) | Same interim placeholder note as the Script variant. |
| `__RTS_FN_NODE_VM_MODULE_HAS_TOP_LEVEL_AWAIT` | `Handle` (module) | `Bool` | Reads a flag recorded at parse time (does the module's own top-level body contain `await`). |
| `__RTS_FN_NODE_VM_MODULE_HAS_ASYNC_GRAPH` | `Handle` (module) | `Bool` | Walks the linked dependency graph; requires prior `instantiate()` (checked in `.ts`, mirroring the documented throw). |
| `__RTS_FN_NODE_VM_MODULE_SYNTHETIC_SET_EXPORT` | `Handle` (module), `StrPtr` (name), `I64` (value, opaque tagged) | `Void` | Backs `syntheticModule.setExport()`. |
| `__RTS_FN_NODE_VM_MODULE_FREE` | `Handle` (module) | `Void` | Releases a module record (and its linked-graph bookkeeping if it's the graph root — exact ownership/refcounting model is an implementation detail for whoever lands this). |
| `__RTS_FN_NODE_VM_MEASURE_MEMORY` | `Bool` (detailed), `Bool` (eager) | `Handle` (Promise resolving to a packed memory-report buffer) | See §5.3 for the promise-settle mechanism and §5.1 for the heap-stats data source. |

Rich values (context/script/module records) are opaque `Handle`s into a
`HandleTable` entry owned by `rts-node`'s own `vm` module — not
`rts-engine`'s primordial `gc::Entry` enum directly (per
`architecture.md` §6, pending the `Entry::Backend(Box<dyn Traceable>)`
extension point). `Buffer`s (cached data, memory-report payloads) are the
ordinary primordial `ArrayBuffer`-backed `Handle`.

### 5.3 Async model

- **Everything in §2 is synchronous except `module.evaluate()` and
  `vm.measureMemory()`.** `createContext`/`isContext`/`runInContext`/
  `runInNewContext`/`runInThisContext`/`compileFunction`/`Script`
  construction+methods/`linkRequests`/`instantiate`/`setExport` are all
  blocking calls with no event-loop involvement.
- **`module.evaluate()`** returns a `Promise` whose settle timing depends on
  whether the module graph has top-level `await` (§4) — this needs the
  **hoisted** promise-settle mechanism (`PromiseSlot` resolve/reject/wait,
  currently `rts-std`, destined for `rts-async` per `architecture.md` §3.2/
  §7) so `rts-node` can allocate and settle a promise without depending on
  `rts-std` directly. For the **no-top-level-`await`** case, the promise
  still settles synchronously from `rts-node`'s own call frame (a promise
  that happens to already be settled by the time it's returned is valid and
  is exactly what real Node documents) — no actual async hop needed in that
  common case; the async hop is only required when top-level `await`
  actually suspends evaluation mid-graph, which additionally needs whatever
  RTS's own `await`/microtask-drain machinery is (same hoisted infra).
- **`vm.measureMemory()`** is promise-shaped purely because V8's
  implementation is async (GC-triggered); RTS's own heap stats read (§5.1) is
  itself synchronous, so RTS can choose to resolve the returned promise
  essentially immediately (optionally still honoring `execution: 'eager'`
  vs `'default'` as "trigger a GC cycle now" vs "wait for the next natural
  one," reusing the mark+sweep collector's own cycle-triggering entry point)
  — needs only the promise-allocation primitive, not a genuine async
  computation.
- **`timeout`/`breakOnSigint`** need a way to interrupt an in-flight
  Cranelift-JIT'd call. RTS's compiled code has no built-in cooperative
  yield/interrupt point comparable to V8's. The most promising angle (**not
  yet confirmed feasible** — flag in §7) is to piggyback the same kind of
  **periodic safepoint check** the GC ticker already threads through compiled
  code (`GC_TICK_INTERVAL`, `crates/rts-runtime/src/namespaces/gc/collector.rs`)
  — i.e. reuse the collector's existing "does the compiled loop check in with
  the runtime periodically" mechanism to also carry a "should this call be
  aborted" flag, rather than inventing a wholly separate interrupt
  infrastructure. Absent that, only a coarse approach is realistic: run the
  target script on a dedicated OS thread and forcibly terminate/abandon that
  thread on timeout — which is unsafe for arbitrary in-flight Rust/engine
  state (allocations, locks) and should be treated as a last-resort,
  explicitly documented limitation, not the default design.
- **`breakOnSigint`** additionally needs to temporarily intercept the
  process's `SIGINT`/Ctrl+C handling for the duration of the call, restoring
  any `process.on('SIGINT')` handlers afterward — ties into whichever module
  owns RTS's own signal handling (likely `node:process`, not built into
  `rts-node`'s `vm` code itself).

### 5.4 Multithread / worker interaction

- **Context/script/module `Handle`s must be safely referenceable from any OS
  thread**, not `thread_local!`-scoped. RTS's async model already hops actual
  OS threads for ordinary work (tokio blocking-pool workers running
  `promise.create`-spawned callbacks, per `docs/specs/async-promise-function.md`)
  — a `vm.Script` compiled on one logical "turn" of the event loop may
  reasonably be `.runInContext()`'d from a different worker thread later. The
  scope/context table (§5.1/§5.2) must therefore be a `Mutex`/shard-guarded
  process-global structure (mirroring the `HandleTable`'s own 32-shard
  design), the **opposite** design choice from e.g. `async_hooks`'s
  deliberately-`thread_local!` context-frame stack — flag this contrast
  explicitly in code comments so a future contributor doesn't "fix" it into a
  `thread_local!` by analogy with that other module.
- **The `timeout`/`breakOnSigint` dedicated execution thread** (§5.3) must be
  registered with `gc/thread_registry` like every other RTS-spawned thread
  that might hold or produce GC-visible handles, since the script it's
  running can allocate ordinary GC-tracked objects.
- **`node:worker_threads` interaction (not yet specced).** Per
  `architecture.md` §8, a `Worker` maps onto an RTS thread with its own
  region. A `vm.Context`/`Script`/`Module` created inside one `Worker` is
  naturally already isolated from another `Worker`'s by virtue of living in
  that worker's own region-scoped state — **no extra work needed** for
  cross-worker isolation as long as the context table above is itself
  partitioned (or at least namespaced) per region. **Contexts/scripts/modules
  are not structured-cloneable in real Node either** — passing a `vm.Context`
  or `vm.Script` handle across a `MessagePort` should throw a
  `DataCloneError`-equivalent, matching Node, rather than RTS inventing a
  transfer/clone semantics Node itself doesn't define.
- **`cluster`/`child_process`** have no special interaction with `vm` beyond
  "a forked/cluster worker process has its own independent RTS process state
  entirely" — no cross-process handle sharing is in scope (handles are never
  meaningful outside the process that allocated them).

### 5.5 Buffer / TypedArray interop

- `cachedData` (accepted by `Script`, `SourceTextModule`, and
  `compileFunction`) is a `Buffer`/`TypedArray`/`DataView` — crosses the ABI
  as a `Handle` into the backing (primordial) `ArrayBuffer`, plus
  byte-offset/length, exactly like every other module's binary-data
  parameters. `createCachedData()`'s return value is a `Buffer` handle
  wrapping freshly-allocated `ArrayBuffer` bytes (empty, per the §5.1
  placeholder, until a real cache format lands).
- `vm.measureMemory()`'s underlying report is read via a small packed
  scratch buffer (`(f64 estimate, f64 low, f64 high)` per context entry, `.ts`
  reassembling the `MemoryMeasurement` shape) rather than N individual boxed
  numbers — the same bulk-dump efficiency pattern used elsewhere in the
  project for small structured native readouts (e.g. `perf_hooks`'s
  `HISTOGRAM_PERCENTILES_DUMP`, `docs/node-implementation/perf_hooks.md`
  §5.2).
- `ModuleRequest.attributes`/`import.meta`/`SyntheticModule` export values are
  arbitrary JS values (may themselves be `Buffer`/`TypedArray`/`ArrayBuffer`)
  — this module never inspects or decodes them, only stores/returns whatever
  opaque tagged value (§5.2) it was given.

### 5.6 Doctrine placement

`node:vm` is **entirely non-primordial** at the naming level: the engine's
front end never names `vm`, `Script`, `Module`, `SourceTextModule`,
`SyntheticModule`, `createContext`, or any other member of this module
anywhere in `crates/rts-codegen-new/`. A `node:vm` import maps via
`ns_prefix_for("node:vm")` (data lookup in `NODE_SPECS`,
`crates/rts-node/src/lib.rs`) to the codegen prefix `node_vm`; calls resolve
generically through `node_lookup`, the same single path every other `node:`
module uses — zero special-case control flow added to the engine on account
of this module existing.

**The one thing worth calling out explicitly, because it's easy to
misread as a doctrine violation and isn't one**: this module's *capability*
(compile-and-run source text, optionally against a substitute scope) is
itself deep **engine** surface — new work for `rts-codegen-new`/the engine's
runtime-compile machinery, not something `rts-node` builds standalone in
`std`-only Rust the way `fs`/`path`/`os` are. That is not a doctrine problem:
the engine's new capability is exposed as a **generic, nameless** primitive
("compile this source, optionally bound to this opaque scope handle, return
an opaque callable/value") — the engine has no idea "vm" or "Script" or
"SourceTextModule" exist; those Node-shaped names live only in `rts-node`'s
`.ts` shim, calling the engine's nameless primitive through the same ordinary
ABI-symbol mechanism every other extern uses, exactly as `new Function(...)`
already does today without the engine knowing which JS class the call
originated from. Contrast with `fs`/`os`/`path` (pure OS wrapping, minimal
engine coupling) — `node:vm` is the opposite extreme: minimal native-OS
surface, maximal (but still doctrine-clean) engine coupling.

`Script`/`Module`/`SourceTextModule`/`SyntheticModule` are `.ts` classes
shipped from `rts-node`'s own `.ts` shim layer (per `architecture.md` §10),
delegating their irreducible operations to the `__RTS_FN_NODE_VM_*` externs
in §5.2. None of them are global — every one exists only inside a file that
actually `import`s `node:vm`.

### 5.7 Shared-infra dependencies (FLAG)

- **The engine's dynamic-compile-against-a-scope-handle primitive (biggest,
  module-specific flag).** Unlike most `node:` modules, `node:vm`'s core
  capability is not something `rts-node` can build unilaterally: only the
  engine (`rts-codegen-new`, which owns Cranelift lowering and identifier
  resolution) can add a "bind free globals to this alternate scope handle"
  mode to its existing runtime-compile primitives. `rts-node` cannot depend
  on `rts-codegen-new` directly — that would be a **cyclic** Cargo
  dependency, since `rts-codegen-new` already depends on `rts-runtime`, which
  (per `architecture.md` §3.2's target layout) depends on `rts-node`. The
  correct shape is the same one `runtime.eval`/`new Function` already use
  today: the engine registers its compile primitive as ordinary
  `abi::SPECS`-style extern symbols; `rts-runtime`'s facade re-exports them;
  `rts-node`'s thin `__RTS_FN_NODE_VM_SCRIPT_*`/`COMPILE_FUNCTION`/`MODULE_*`
  externs call through to those **already-linked** symbols via the harvested
  JIT-symbol-table mechanism (`adapter_symbols`), not a Rust `use` — so no
  new Cargo edge is needed in either direction. What *is* needed is a
  **coordination ask on whoever owns the engine's compile pipeline**: grow it
  to accept an optional scope/context handle, and to register a few new,
  more general symbols (context-scoped compile+run; unlinked module-record
  construction) alongside the existing ones. Until that lands, only
  `vm.compileFunction()` (no `parsingContext`) and `vm.runInThisContext()`
  are honestly implementable as thin wrappers (§5.8 phases b/c); everything
  context-scoped is blocked on this.
- **Hoisted promise-settle infra (`rts-async`).** `module.evaluate()` (the
  top-level-`await` case) and `vm.measureMemory()` both need to
  allocate/settle a `Promise` without depending on `rts-std` directly — the
  same hoist `docs/node-implementation/architecture.md` §7 and every other
  async-touching module doc (`module.md` §5.3, `perf_hooks.md` §5.7) already
  flags. Until it lands, `module.evaluate()` is only implementable for the
  synchronous (no-top-level-`await`) fulfillment case, and
  `vm.measureMemory()` cannot return a real `Promise` at all.
- **RTS's own dynamic module-loading capability (shared with `node:module`
  `#223`).** `SourceTextModule`'s unlinked-record contract is the same
  "RTS resolves imports statically; Node needs a dynamic loader"
  architecture mismatch `docs/node-implementation/module.md` §5.1/§5.7/§7
  raises for the `#223` dynamic-import epic — coordinate sequencing with
  whoever owns that work rather than solving it twice.
- **GC/heap statistics surface for `vm.measureMemory()`.** Needs
  `rts-engine`'s mark+sweep collector to expose whatever per-allocation size
  accounting `node:v8`'s `getHeapStatistics()` will also need
  (`architecture.md` §11) — a shared read, not a new subsystem; coordinate
  with whoever specs/builds `node:v8` so the two don't duplicate the stats
  plumbing.
- **`SIGINT` handling for `breakOnSigint`.** Needs whatever `node:process`
  ends up using for `process.on('SIGINT')`/Ctrl+C translation (not yet
  specced) — a wiring dependency on that module's eventual signal-handling
  primitive, not a new subsystem `node:vm` should build itself.
- If none of the above is wired, this module can still ship a **reduced**
  honest slice: `vm.compileFunction()` (current-scope only),
  `vm.runInThisContext()`, `new vm.Script(code).runInThisContext()`, and
  `vm.constants` — all needing only the *already-existing* engine
  runtime-compile primitive, zero new engine work, zero hoist. Everything
  context-scoped, module-record-based, timeout/interrupt-based, or
  memory-measurement-based is gated on one or more of the flags above.

### 5.8 Implementation phases

a. **`vm.constants`** (`USE_MAIN_CONTEXT_DEFAULT_LOADER`, `DONT_CONTEXTIFY` —
   both plain `Symbol()` values, primordial) as a pure `.ts` slice. Zero
   native work.
b. **`vm.compileFunction(code, params)`** in the current/global scope only
   (no `parsingContext`/`contextExtensions`/`cachedData`) — thin wrapper over
   the engine's existing dynamic-compile primitive (§5.1/§5.7). Establishes
   the `rts-node` → already-published-symbol calling convention with **zero**
   new engine work.
c. **`vm.Script` (construct + `runInThisContext()` only)** — same underlying
   primitive, wrapped in the `Script` class shape. `cachedDataRejected`
   always `true`/`undefined` placeholder (§5.1); `sourceMapURL` best-effort
   magic-comment scan; `createCachedData()` returns an empty-`Buffer`
   placeholder, clearly flagged non-functional.
d. **`vm.createContext()`/`vm.isContext()`/`script.runInContext()`/
   `vm.runInContext()`** — the real new-capability slice, blocked on the
   engine's scope-handle primitive landing (§5.7). Needs an owner decision:
   block this phase entirely until the engine capability exists, or ship the
   explicitly-flagged property-bag-merge interim approximation (§5.1 item 2
   option b) in the meantime. **Recommendation: block, don't ship the
   semantically-wrong interim** — a "context" that silently leaks top-level
   `var`/`function` declarations into the real global scope is a worse trap
   than an honest "not implemented" error.
e. **`vm.runInNewContext()` / `script.runInNewContext()`** — shortcuts
   composing (c) + (d); no new capability once both exist.
f. **`timeout` / `breakOnSigint` support** — dedicated OS thread +
   coarse/best-effort interruption (§5.3/§5.4); investigate the
   GC-safepoint-piggyback angle first (§5.3) before committing to the unsafe
   thread-kill fallback. Document whatever granularity is actually achieved.
g. **`cachedData`/`createCachedData()` real implementation** — once RTS
   settles on its own artifact-cache format (tie to `module.md`'s
   compile-cache-identity open question, §5.1/§5.7).
h. **`vm.measureMemory()`** — wraps `rts-engine` heap/`HandleTable` stats;
   needs the hoisted promise-settle infra (§5.7). `'summary'` mode first;
   `'detailed'` (per-context) only once contexts are real handles (phase d).
i. **`vm.Module`/`vm.SourceTextModule`/`vm.SyntheticModule`** (Experimental
   stability in real Node too) — the single biggest remaining item.
   `SyntheticModule` first (no parsing, just fixed export slots + a
   callback — comparatively easy). `SourceTextModule` next, needs the
   dynamic-module-record capability shared with `node:module`'s `#223`
   blocker (§5.7): `status`/`identifier`/`error`/`namespace`,
   `link()`/`linkRequests()`/`instantiate()`, `evaluate()`,
   `moduleRequests`/`dependencySpecifiers`, `hasTopLevelAwait()`/
   `hasAsyncGraph()`, `createCachedData()` (reusing (g)'s format decision).
   Defer to last given both its Experimental stability ceiling and its
   dependency on unshipped shared infra.

## 6. Test plan

Fixtures live in `tests/*.test.ts` (`rts:test` format, per project convention).
All examples below assume `import * as vm from 'node:vm';` (or the named
imports actually used).

1. **`compileFunction` happy path** — `vm.compileFunction('return a + b;', ['a', 'b'])`, call the returned function with `(2, 3)`, assert `5`. Also test zero-`params` form and a body with an internal `const`/`let` (verifying it doesn't leak to the caller's scope).
2. **`compileFunction` filename in a thrown error** — compile a body that throws, pass `filename: 'my-file.vm'`, assert the error's `.stack` mentions it (best-effort; skip/soft-assert if RTS's stack formatting differs — document the gap rather than hard-fail).
3. **`new Script(code).runInThisContext()` basic** — `new vm.Script('1 + 1').runInThisContext()` returns `2`; a multi-statement body returns the **last** statement's value.
4. **No access to caller's local scope** — inside a function with a local `const secret = 42;`, run `new vm.Script('typeof secret').runInThisContext()` and assert the result is `'undefined'`, not `42`.
5. **`runInThisContext` sees the real globals** — set `globalThis.__probe = 7` beforehand, run `new vm.Script('__probe').runInThisContext()`, assert `7`.
6. **`createContext` + `runInContext` — contextify semantics** — `const ctx = vm.createContext({ x: 1 })`; `vm.runInContext('x += 41; x', ctx)` returns `42`; afterward assert `ctx.x === 42` (write-back onto the context object) **and** assert the real `globalThis.x` is untouched (isolation).
7. **`isContext()`** — `vm.isContext(ctx)` is `true` for a `createContext()`-produced object; `vm.isContext({})` (a plain, never-contextified object) is `false`.
8. **`runInNewContext` shortcut equivalence** — `vm.runInNewContext('x*2', { x: 21 })` returns `42` in one call, equivalent to manually chaining `createContext` + `runInContext`.
9. **`vm.constants.DONT_CONTEXTIFY` + freeze** — `const ctx = vm.createContext(vm.constants.DONT_CONTEXTIFY); vm.runInContext('Object.freeze(globalThis); 1+1', ctx)` does not throw and returns `2`; a subsequent attempt to run `'x = 5'` (a fresh global assignment) against the same frozen `ctx` throws (frozen global object rejects a new property).
10. **`displayErrors`** — a script with a syntax error thrown at construction always carries source context; a **runtime** throw run with `displayErrors: false` vs default `true` — assert the error is thrown either way, soft-assert on the exact stack content difference (documented as an approximation, §5.1).
11. **`timeout`** — `vm.runInNewContext('while(true){}', {}, { timeout: 50 })` throws within a bounded wall-clock window (test with a generous upper bound, e.g. assert it throws before 5s) and the thrown error is recognizable as a timeout (message or a documented RTS-specific marker, since `ERR_SCRIPT_EXECUTION_TIMEOUT` may not be pinned bit-for-bit yet — assert on documented behavior, not Node's exact error object shape, until §5.8(f) lands for real).
12. **`Script.cachedDataRejected`/`createCachedData()` interim behavior** — call `createCachedData()`, assert it returns a `Buffer` (possibly empty, per the documented §5.1 placeholder) without throwing; construct a second `Script` with that buffer as `cachedData` and assert `cachedDataRejected === true` (honest placeholder behavior) — update this fixture's expectation once §5.8(g) ships a real cache format.
13. **`SyntheticModule` (behind whatever RTS flag/always-on gate is chosen, §4)** — `new vm.SyntheticModule(['default'], function() { this.setExport('default', 42); })`; `await mod.link(() => { throw new Error('no deps'); })` (never invoked, no dependencies) or `mod.linkRequests([])` + `mod.instantiate()`; `await mod.evaluate()`; assert `mod.namespace.default === 42` and `mod.status === 'evaluated'`.
14. **`SourceTextModule`, no dependencies** — `new vm.SourceTextModule('export const x = 1 + 1;')`; `linkRequests([])` + `instantiate()`; `await evaluate()`; assert `module.namespace.x === 2` and the full status sequence `'unlinked' → 'linked' → 'evaluated'` was observed (poll/log `status` at each step).
15. **`SourceTextModule` with one dependency, via a custom `linker`** — module A imports from `'dep'`; module B is a pre-built `SyntheticModule` exporting `value = 10`; `await A.link((specifier) => specifier === 'dep' ? B : Promise.reject(new Error('unknown')))`; evaluate A; assert A's namespace re-export (`export { value } from 'dep'`, or A reading `dep`'s export internally) reflects `10`.
16. **`sourceTextModule.moduleRequests` shape** — a module with two `import` statements (one plain, one `with { type: 'json' }`-shaped attribute) — assert the array has 2 entries with the right `specifier`/`attributes`/`phase` (exact `phase` value per §4's worked example).
17. **`hasTopLevelAwait()` / `hasAsyncGraph()`** — one module with a bare `await` at its top level → `hasTopLevelAwait() === true`; a second module that only *imports* the first (no `await` itself) → `hasTopLevelAwait() === false` but `hasAsyncGraph() === true` after instantiation; assert calling `hasAsyncGraph()` **before** `instantiate()` throws.
18. **`vm.measureMemory()` summary** — `const r = await vm.measureMemory();` assert `typeof r.total.jsMemoryEstimate === 'number'` and it's `>= 0`; assert `r.current`/`r.other` are `undefined` in summary mode (default).
19. **`vm.measureMemory({ mode: 'detailed' })`** — assert `r.current` is present and `r.other` is an array (possibly empty in a single-context RTS process until real multi-context tracking lands, §5.8(h) — soft-assert shape, not exact contents).
20. **Multithread: context handle shared across two OS threads** — using RTS's own `thread.spawn`/`spawn_async_join` primitive, create one `vm.createContext()` on the main thread, then run `vm.runInContext('x++', ctx)` concurrently from two spawned threads a fixed number of times each; assert the final `ctx.x` reflects all increments with no crash/data race (validates §5.4's Handle-based, non-`thread_local!` design) — mark this fixture explicitly as exercising §5.4, not a Node-parity behavior test (Node itself is single-threaded per instance and has no equivalent scenario).
21. **Error thrown inside `runInContext` propagates as a real JS exception** — `vm.runInContext('throw new TypeError("boom")', ctx)` wrapped in `try/catch`, assert `e instanceof TypeError` and `e.message === 'boom'`.
22. **`ERR_MODULE_LINK_MISMATCH`** — construct a module with two import requests for the same specifier + identical attributes, call `linkRequests([modA, modB])` with **two different** module instances for those two requests, assert it throws (or rejects, matching whatever surface RTS settles on) recognizably.

## 7. Open questions / deferrals

- **The scope-substitution mechanism itself (biggest open item).** Whether
  "a vm context" becomes a first-class engine concept (a scope/region handle
  threaded through the compile pipeline, §5.1 option 1) or something else
  entirely is an open engine-design question, not something this spec can
  settle unilaterally — needs sign-off from whoever owns
  `rts-codegen-new`/the compile pipeline before phase (d) can start for real.
- **Interrupting a running compiled call (`timeout`/`breakOnSigint`).**
  The GC-safepoint-piggyback idea (§5.3) is a plausible angle, **not a
  confirmed-feasible design** — needs a spike before committing to it over
  the unsafe thread-kill fallback.
- **Cache format for `cachedData`/`createCachedData()`.** No natural 1:1
  analogue to V8's bytecode cache exists in a Cranelift-JIT world; whether to
  reuse/extend the existing `.ometa` artifact-cache format or invent a
  narrower vm-specific blob is undecided — the same open question
  `docs/node-implementation/module.md` raises for `enableCompileCache`;
  should likely be answered once, for both modules together.
- **`--experimental-vm-modules`-equivalent gating.** Whether RTS should
  bother replicating Node's flag-gate for `vm.Module`/`SourceTextModule`/
  `SyntheticModule` (given RTS ships its own release cadence and doesn't need
  to preserve V8's own experimental/stable API split reasoning) is an open
  product decision — defaulting to "always available, documented as
  Experimental stability" is the simplest option absent a reason to gate.
- **`importModuleDynamically`/`vm.constants.USE_MAIN_CONTEXT_DEFAULT_LOADER`.**
  Fully depends on RTS gaining a genuine dynamic `import()` capability
  (`#223`) — out of scope for this module alone; revisit once that epic has
  its own implementation, at which point this module's `importModuleDynamically`
  options become simple pass-throughs to it.
- **`SourceTextModule`'s `initializeImportMeta`/`import.meta` semantics under
  RTS's static import model.** Even once basic module-record creation works
  (§5.8(i)), whether `import.meta` inside a vm-compiled module can be
  meaningfully distinct per-instantiation (as Node's docs imply) without the
  same dynamic-loader capability is unclear — likely deferred alongside
  `#223` too.
- **`vm.measureMemory()` `'detailed'` mode's real multi-context breakdown.**
  Meaningless until vm contexts are real, independently-trackable engine
  scopes (phase d) — until then, `'detailed'` mode can only honestly report a
  single-entry `other: []`/a `current` identical to `total`.
- **Snapshot APIs (`v8.startupSnapshot`) and any `vm`/snapshot interaction.**
  Intentionally out of scope for this doc — belongs to `node:v8`; noted only
  so a future reader doesn't expect this doc to cover it.
- **CDP/Inspector visibility of `vm` context `name`/`origin`.** Node surfaces
  these through the Inspector protocol; RTS's own debug/inspector story
  (`docs/node-implementation/inspector.md`) is itself a documented deferral —
  this module should not block on it, just store `name`/`origin` faithfully
  in case a future inspector surface wants to read them.
