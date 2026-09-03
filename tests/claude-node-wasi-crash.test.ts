// node:wasi — `WASI({ args: [...] })` and `WASI({ env: {...} })` (and, by the
// same code path, `preopens`) do not answer `undefined` or throw a catchable
// JS error. They PANIC THE WHOLE PROCESS: `[RTS PANIC] RefCell already
// borrowed`, an actual Rust abort, not a JS-level failure — the single
// severest finding across all four modules this session covered. Isolated
// here so `claude-node-wasi.test.ts` stays measurable (every `WASI(...)`
// call there deliberately omits `args`/`env`/`preopens`).
//
// Root cause, read directly from the source: `constructor` in
// `crates/rts-node/src/wasi/mod.rs` calls `read_string_array(context, options,
// "args")` and `read_string_map(context, options, "env"/"preopens")` from
// INSIDE an `entry::with_runtime(|context| ...)` closure — i.e. while the
// engine's per-thread `Context` `RefCell` is already borrowed. Both helpers
// take that same `context` as a parameter (so far so normal), but each ALSO
// calls an AMBIENT entry point that borrows the context a SECOND time itself:
//
//   - `read_string_array` calls `entry::get_indexed(array, index)` per
//     element — `get_indexed` is ambient (it calls `with_current`/
//     `with_runtime` on its own), so calling it from inside an ALREADY-HELD
//     borrow is exactly the nested-borrow shape this crate's own convention
//     names as fatal elsewhere. `sqlite/database.rs`'s `option_raw` doc
//     describes the identical hazard and the two-step fix (decode the raw
//     value AFTER the borrow that fetched it ends) — that fix was applied
//     there and was NOT applied here.
//   - `read_string_map` calls `entry::own_keys(map)` — also ambient, same
//     hazard — to walk an options object's keys (`env`, `preopens`).
//
// So this is not specific to `args` vs `env`: it is the SAME bug in two
// sibling helpers, both reached from the one constructor, both triggered by
// the two most ordinary options `node:wasi`'s own documentation shows first.
// A `WASI` instance can only safely be built with `version`/`stdin`/`stdout`/
// `stderr`/`returnOnExit` — the fields read by `read_string`/`read_i32`/
// `read_bool`, none of which call an ambient helper.
//
// Reproduced identically via `rts run` on a two-line script outside the test
// harness (both for `args` alone and `env` alone) before being written here.
import { describe, test, expect } from "rts:test";
import * as wasiMod from "node:wasi";

// The plain-call workaround for the SEPARATE `new WASI(...)` bug (see
// `claude-node-wasi.test.ts`) is used here too, though it makes no
// difference to this crash — `new WASI({ args: [...] })` panics identically.
const WASI: any = (wasiMod as any).WASI;

describe("node:wasi — args/env/preopens options", () => {
    test("{ args: [...] } does not panic the process", () => {
        // THE KILLING LINE.
        const w = WASI({ version: "preview1", args: ["prog", "a", "b"] });
        expect(typeof w).toBe("object");
    });

    test("{ env: {...} } does not panic the process", () => {
        const w = WASI({ version: "preview1", env: { FOO: "bar" } });
        expect(typeof w).toBe("object");
    });
});
