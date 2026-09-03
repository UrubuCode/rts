// node:async_hooks — the ONE call that kills the process: `new AsyncLocalStorage()`.
//
// crates/rts-node/src/async_hooks/local.rs's `construct` (used by BOTH `new
// AsyncLocalStorage()` and `als.run(...)`-independent construction) calls
// `rts_core::entry::make_prototype(context, "AsyncLocalStorage", METHODS)`
// directly, from local.rs. But `mod.rs`'s `namespace()` already registered
// the SAME name ("AsyncLocalStorage") through `attach()`, which is defined
// in and therefore "owned" by mod.rs, at namespace-build time (before any
// program code runs). `make_prototype`'s collision guard keys ownership by
// the CALLER'S SOURCE FILE (see resource.rs's own comment on this exact
// mechanism), so `local.rs` calling it a second time under the identical
// name reads as two DIFFERENT modules fighting over one prototype name and
// panics the whole process — even though it is the same logical class,
// registered once at install time and then AGAIN at first construction.
//
// This is the exact defect class `resource.rs`'s `install()` already patched
// for `AsyncResource` (its own comment: "Twelve files of Node's own
// async_hooks suite died that way, measured 2026-08-24") — by minting the
// prototype ONCE in `install()` and caching it in a `Cell`, then having
// `construct()` read the cached prototype instead of calling
// `make_prototype` again. `local.rs`'s `construct()` was never given the
// same fix: it still calls `make_prototype` itself, so `new
// AsyncLocalStorage()` is unusable in this build.
//
// The minimal repro is exactly two lines: import the class, construct one.
// No test() bodies run — the RTS PANIC happens while evaluating the
// top-level module code, so this file's own `test()` calls never execute at
// all. `target/fast/rts.exe run` on this file exits 1 with:
//
//   [RTS PANIC] make_prototype("AsyncLocalStorage") collision: already
//   owned by crates\rts-node\src\async_hooks\mod.rs, also claimed by
//   crates\rts-node\src\async_hooks\local.rs — two modules registered
//   different method tables under one prototype name; rename one (e.g.
//   "module.AsyncLocalStorage") or, if the sharing is deliberate and
//   self-healing, add it to SHARED_BY_DESIGN with the same reasoning as its
//   existing entries
//     at crates\rts-node\src\async_hooks\local.rs:230:13
//
// Every AsyncLocalStorage-dependent scenario this task asked for (run/
// getStore/exit/enterWith/withScope, the static bind/snapshot capture-order
// trap) is therefore UNMEASURABLE against this build: none of it can run
// because the class cannot be constructed at all. See
// tests/claude-node-async-hooks.test.ts for what DOES run (AsyncResource,
// which is unaffected — its own install() already caches its prototype the
// way this file's fix needs to).
import { AsyncLocalStorage } from "node:async_hooks";

const als = new AsyncLocalStorage();
console.log("unreachable — new AsyncLocalStorage() panics the process before this line");
