// node:wasi — `crates/rts-node/src/wasi/`, over `wasmi`. Nothing in this
// repository's own fixtures had ever constructed a `WASI` instance before
// this file. The module's own stated divergence — `start`/`initialize` take
// a WASM module's RAW BYTES (a `Uint8Array`), not a `WebAssembly.Instance`,
// because this engine has no `WebAssembly` global at all — is honoured
// below: every module exercised here is a hand-built byte array, small
// enough to write out in full, and EACH ONE was independently validated as
// real, loadable WebAssembly first, using real Node's own `WebAssembly`
// (`new WebAssembly.Module(bytes)` / `new WebAssembly.Instance(...)`, which
// IS available in the Node v20 on this machine even though `node:wasi`
// itself is not) — so a failure below is this engine's, not a malformed
// fixture's.
//
// TWO SEPARATE, SEVERE BUGS surfaced while writing this file, both filed
// here rather than hidden:
//
// 1. `new WASI(options)` — the DOCUMENTED, only way to construct one —
//    always answers `undefined`, for ANY options including none at all.
//    Calling the SAME exported function WITHOUT `new` — `WASI(options)`,
//    an ordinary call — answers the correct instance every time. Root cause,
//    read from the source: `wasi::namespace` installs `WASI` as an ordinary
//    member of `entry::make_namespace`'s list (`[("WASI", constructor)]`),
//    the same shape every plain namespace FUNCTION uses — where
//    `sqlite::database::construct` and `vm`'s `script_ctor` are instead
//    wired as their own `entry::make_callable` handed to `put_member`
//    directly, and BOTH of those work correctly under `new`. Something about
//    a namespace-list-installed function specifically breaks the `new`
//    protocol; every test below therefore constructs with the plain-call
//    workaround, each time noted, so the REST of the module is still
//    measurable.
// 2. `new WASI({ args: [...] })` and `new WASI({ env: {...} })` — and, by
//    the same code path, `preopens` — do not answer `undefined` or throw a
//    JS error at all: they PANIC THE PROCESS with "RefCell already
//    borrowed", a genuine nested-borrow abort of the exact shape this
//    crate's own convention names as fatal elsewhere (see
//    `sqlite/database.rs`'s `option_raw` doc on the identical hazard, handled
//    correctly there). Isolated in `claude-node-wasi-crash.test.ts` — every
//    `WASI(...)` call in THIS file omits `args`/`env`/`preopens` for that
//    reason.
import { describe, test, expect } from "rts:test";
import * as wasiMod from "node:wasi";

// `new` is broken (see comment above) — every construction below is a plain
// call, which IS the working shape, used deliberately as a workaround.
const WASI: any = (wasiMod as any).WASI;

// ── BUG: `new WASI(...)` ─────────────────────────────────────────────────────
const ctorViaNew = new WASI({ version: "preview1" });
const ctorViaCall = WASI({ version: "preview1" });

// ── getImportObject() / wasiImport ───────────────────────────────────────────
const w1 = WASI({ version: "preview1" });
const io = w1.getImportObject();
const ioHasPreview1Namespace = typeof io.wasi_snapshot_preview1 === "object";
const procExitIsFunction = typeof io.wasi_snapshot_preview1.proc_exit === "function";
// The module doc's own comment on `INERT_SYSCALL_NAMES` says this should
// match "what a program iterating Object.keys(wasiImport) would see" — it
// does not: every native namespace in this engine (checked: `node:worker_threads`'
// own module namespace shows the identical shape) answers an EMPTY array from
// `Object.keys`, even though each name is directly reachable by property
// access. This is a doc-vs-actual mismatch worth naming, not this module's
// bug specifically.
const wasiImportKeysViaObjectKeys = Object.keys(io.wasi_snapshot_preview1);
// RED, discovered while writing this: `build_import_object` runs fresh on
// EVERY call — at construction (for `wasiImport`) and again inside
// `getImportObject()` — so this is a freshly made namespace object each
// time rather than a cached one, and the two are never reference-equal.
// Node's own documented contract is IDENTITY:
// `result.wasi_snapshot_preview1 === wasi.wasiImport`.
const wasiImportSameAsGetImportObject = w1.wasiImport === io.wasi_snapshot_preview1;
// Calling an inert stand-in directly does nothing useful (documented) — it
// answers `undefined` rather than running a real syscall.
const inertCallResult = io.wasi_snapshot_preview1.proc_exit(9);

// ── finalizeBindings — refused, answers undefined ────────────────────────────
const finalizeBindingsResult = w1.finalizeBindings();

// ── start(): a real, minimal, hand-assembled WASM module ────────────────────
// `(module (memory (export "memory") 1) (func (export "_start")))` — 50
// bytes, validated with real Node's `new WebAssembly.Module(...)` before
// being written here (a valid module with no imports, no work: `_start`
// just returns).
const tinyStartModule = new Uint8Array([
    0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0, 5, 3, 1, 0, 1, 7, 19, 2, 6, 109, 101, 109, 111, 114,
    121, 2, 0, 6, 95, 115, 116, 97, 114, 116, 0, 0, 10, 4, 1, 2, 0, 11,
]);
const wStart = WASI({ version: "preview1" });
const startResult = wStart.start(tinyStartModule);
// call-once: a second `start`/`initialize` on the SAME instance is refused.
const startAgainResult = wStart.start(tinyStartModule);

// ── start(): a module that imports `proc_exit` and calls it with 42 ─────────
// `(import "wasi_snapshot_preview1" "proc_exit" (func (param i32)))`, then
// `_start` does `i32.const 42; call $proc_exit`. 96 bytes, validated the same
// way (including running it under real `WebAssembly` with a stub `proc_exit`
// that confirms it is actually invoked with 42).
const procExitModule = new Uint8Array([
    0, 97, 115, 109, 1, 0, 0, 0, 1, 8, 2, 96, 0, 0, 96, 1, 127, 0, 2, 36, 1, 22, 119, 97, 115, 105, 95, 115, 110, 97,
    112, 115, 104, 111, 116, 95, 112, 114, 101, 118, 105, 101, 119, 49, 9, 112, 114, 111, 99, 95, 101, 120, 105, 116,
    0, 1, 3, 2, 1, 0, 5, 3, 1, 0, 1, 7, 19, 2, 6, 109, 101, 109, 111, 114, 121, 2, 0, 6, 95, 115, 116, 97, 114, 116,
    0, 1, 10, 8, 1, 6, 0, 65, 42, 16, 0, 11,
]);
const wExit = WASI({ version: "preview1" });
const exitCodeResult = wExit.start(procExitModule);

// ── start(): malformed/no-export bytes answers undefined, not a throw ───────
const wBad = WASI({ version: "preview1" });
const badBytesResult = wBad.start(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0])); // header only, no exports

// ── initialize(): the reactor entry point, `_initialize` instead of `_start` ─
// Same shape as `tinyStartModule` but exporting `_initialize`. 54 bytes.
const tinyInitModule = new Uint8Array([
    0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0, 5, 3, 1, 0, 1, 7, 24, 2, 6, 109, 101, 109, 111, 114,
    121, 2, 0, 11, 95, 105, 110, 105, 116, 105, 97, 108, 105, 122, 101, 0, 0, 10, 4, 1, 2, 0, 11,
]);
const wInit = WASI({ version: "preview1" });
const initResult = wInit.initialize(tinyInitModule);
// Node's own `initialize` has no return value — `undefined` here matches
// Node, not merely this engine's inability to answer otherwise.

// entry-point mismatch: calling `start()` on a reactor-only module (no
// `_start` export) is refused, same as bad bytes.
const wMismatch = WASI({ version: "preview1" });
const startOnReactorOnlyResult = wMismatch.start(tinyInitModule);

// ── the REAL host syscalls, exercised by running actual WASM through them ───
// These are NOT `wasiImport`'s inert stand-ins — they are `host.rs`'s actual
// `wasmi::Linker`-wired functions, reached the only way JS can reach them:
// by running a module that imports and calls one, then reports the answer
// back via `proc_exit`'s own exit code. `(i32,i32)->i32` for
// `fd_prestat_get`, called as `fd_prestat_get(3, 0)`. 146 bytes, validated
// against real `WebAssembly` first.
const fdPrestatGetModule = new Uint8Array([
    0, 97, 115, 109, 1, 0, 0, 0, 1, 14, 3, 96, 0, 0, 96, 2, 127, 127, 1, 127, 96, 1, 127, 0, 2, 76, 2, 22, 119, 97,
    115, 105, 95, 115, 110, 97, 112, 115, 104, 111, 116, 95, 112, 114, 101, 118, 105, 101, 119, 49, 14, 102, 100, 95,
    112, 114, 101, 115, 116, 97, 116, 95, 103, 101, 116, 0, 1, 22, 119, 97, 115, 105, 95, 115, 110, 97, 112, 115,
    104, 111, 116, 95, 112, 114, 101, 118, 105, 101, 119, 49, 9, 112, 114, 111, 99, 95, 101, 120, 105, 116, 0, 2, 3,
    2, 1, 0, 5, 3, 1, 0, 1, 7, 19, 2, 6, 109, 101, 109, 111, 114, 121, 2, 0, 6, 95, 115, 116, 97, 114, 116, 0, 2, 10,
    12, 1, 10, 0, 65, 3, 65, 0, 16, 0, 16, 1, 11,
]);
const wFdPrestat = WASI({ version: "preview1" });
const fdPrestatErrno = wFdPrestat.start(fdPrestatGetModule); // expect 8 (EBADF)

// `(i32,i64,i32,i32)->i32` for `fd_seek`, called as `fd_seek(3, 0, 0, 0)`.
// 145 bytes, same validation.
const fdSeekModule = new Uint8Array([
    0, 97, 115, 109, 1, 0, 0, 0, 1, 16, 3, 96, 0, 0, 96, 4, 127, 126, 127, 127, 1, 127, 96, 1, 127, 0, 2, 69, 2, 22,
    119, 97, 115, 105, 95, 115, 110, 97, 112, 115, 104, 111, 116, 95, 112, 114, 101, 118, 105, 101, 119, 49, 7, 102,
    100, 95, 115, 101, 101, 107, 0, 1, 22, 119, 97, 115, 105, 95, 115, 110, 97, 112, 115, 104, 111, 116, 95, 112,
    114, 101, 118, 105, 101, 119, 49, 9, 112, 114, 111, 99, 95, 101, 120, 105, 116, 0, 2, 3, 2, 1, 0, 5, 3, 1, 0, 1,
    7, 19, 2, 6, 109, 101, 109, 111, 114, 121, 2, 0, 6, 95, 115, 116, 97, 114, 116, 0, 2, 10, 16, 1, 14, 0, 65, 3, 66,
    0, 65, 0, 65, 0, 16, 0, 16, 1, 11,
]);
const wFdSeek = WASI({ version: "preview1" });
const fdSeekErrno = wFdSeek.start(fdSeekModule); // expect 52 (ENOSYS)

describe("node:wasi — new WASI(...) (RED: real bug, see file header)", () => {
    test("`new WASI(options)` should answer an instance", () => expect(typeof ctorViaNew).toBe("object"));
    test("...the plain-call workaround does", () => expect(typeof ctorViaCall).toBe("object"));
});

describe("node:wasi — getImportObject() / wasiImport", () => {
    test("has a wasi_snapshot_preview1 namespace", () => expect(ioHasPreview1Namespace).toBe(true));
    test("proc_exit is present as a function", () => expect(procExitIsFunction).toBe(true));
    test("calling an inert stand-in directly does nothing (documented)", () =>
        expect(inertCallResult).toBe(undefined));
});

describe("node:wasi — wasiImport identity (RED: real bug, see comment above)", () => {
    test("wasiImport IS the same object getImportObject() answers — Node's documented contract", () =>
        expect(wasiImportSameAsGetImportObject).toBe(true));
});

describe("node:wasi — Object.keys(wasiImport) (RED: contradicts the module's own comment)", () => {
    test("Object.keys should list the syscall names, per the module's own INERT_SYSCALL_NAMES comment", () =>
        expect(wasiImportKeysViaObjectKeys.length > 0).toBe(true));
});

describe("node:wasi — finalizeBindings (refused, documented)", () => {
    test("answers undefined rather than throwing", () => expect(finalizeBindingsResult).toBe(undefined));
});

describe("node:wasi — start(), a real minimal module", () => {
    test("runs to completion with exit code 0", () => expect(startResult).toBe(0));
    test("a second start() on the same instance is refused (call-once)", () =>
        expect(startAgainResult).toBe(undefined));
});

describe("node:wasi — start(), proc_exit() inside the module", () => {
    test("the module's own proc_exit(42) becomes start()'s answer", () => expect(exitCodeResult).toBe(42));
});

describe("node:wasi — start(), malformed bytes", () => {
    test("header-only bytes (no _start export) answer undefined, not a throw", () =>
        expect(badBytesResult).toBe(undefined));
});

describe("node:wasi — initialize(), the reactor entry point", () => {
    test("runs _initialize and answers undefined (matches Node's own contract)", () =>
        expect(initResult).toBe(undefined));
    test("start() on a module with no _start export is refused the same way", () =>
        expect(startOnReactorOnlyResult).toBe(undefined));
});

describe("node:wasi — REAL filesystem syscalls answer their documented errno", () => {
    test("fd_prestat_get(3, 0) answers EBADF (8)", () => expect(fdPrestatErrno).toBe(8));
    test("fd_seek(...) answers ENOSYS (52)", () => expect(fdSeekErrno).toBe(52));
});
