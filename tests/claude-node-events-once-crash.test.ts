// node:events — events.once(emitter, name) CRASHES THE PROCESS every time
// the awaited event actually fires (the success path — the whole reason the
// function exists). This file holds only that one killer call, isolated, so
// the rest of `events.once`'s surface stays measurable in
// claude-node-events-once.test.ts.
//
// ROOT CAUSE (found by isolating the exact line, then reading the source):
// `once_promise.rs`'s `on_event` — the closure that settles the promise when
// the awaited event fires — calls `super::packed_args(a0, a1, a2)` FROM
// INSIDE an `entry::with_runtime(|context| ...)` closure:
//
//   let (promise, args) = entry::with_runtime(|context| (
//       entry::get_member(context, state, "promise"),
//       super::packed_args(a0, a1, a2),          // <-- here
//   ));
//
// `packed_args` (in events/mod.rs) itself calls the AMBIENT
// `entry::undefined_value()`, which takes the runtime borrow itself — a
// second, nested borrow of the same RefCell the enclosing `with_runtime` is
// already holding open. That is exactly the hazard the module's own doc
// warns about in its "borrow every module here has to get right" section,
// and it is not hypothetical: it is Rust's `RefCell` refusing the second
// borrow and panicking, which aborts the whole process (exit code 127 here,
// not a catchable JS exception) rather than failing one test.
//
// `on_iterator.rs`'s own `on_event` (used by `events.on`, not `events.once`)
// calls `packed_args` OUTSIDE any `with_runtime` block, which is why
// `events.on` does NOT hit this — see claude-node-events-on.test.ts, which
// exercises exactly that non-crashing sibling.
//
// CONFIRMED shape of the crash, isolated by hand before writing this file:
//   - `events.once(e, "x")` then `e.emit("x", 1, 2)` on the very next line,
//     fully synchronous, no `await` anywhere yet — crashes.
//   - `events.once(e, "error")` then `e.emit("error", err)` — crashes too:
//     when the AWAITED name itself is "error", the success listener
//     (`on_event`) is what fires, not the separate error-listener, so it
//     hits the same bug.
//   - `events.once(e, "x")` then a `setTimeout(() => e.emit("x", …), 5)`
//     BEFORE the `await` — still crashes once the timer fires and pumps the
//     await forward. Not a same-turn artifact; every successful resolution
//     hits it.
//   - By contrast, `events.once(e, "data")` rejecting via a DIFFERENT
//     event ('error', or an aborted signal) does NOT crash — those settle
//     through `on_error`/`on_abort`, neither of which calls `packed_args`.
//     See claude-node-events-once.test.ts for that half.
//
// Real Node, for comparison (`node -e`): `events.once(e, "x")` then
// `e.emit("x", 1, 2)` then `await`ing it answers `[1, 2]` — ordinary,
// unremarkable, and is what the assertion below states as the expected
// (Node) answer. It cannot pass on RTS today: the process is gone before
// the assertion runs.
import { describe, test, expect } from "rts:test";
import { EventEmitter } from "node:events";
import * as events from "node:events";

const e = new EventEmitter();
const p = events.once(e, "x");
e.emit("x", 1, 2); // <-- the killer call: crashes the process, RefCell already borrowed

describe("node:events events.once — success path (CRASHES on RTS)", () => {
    test("resolves with the emitted argument array (Node's answer)", async () => {
        const result = await p;
        expect(result.length).toBe(2);
        expect(result[0]).toBe(1);
        expect(result[1]).toBe(2);
    });
});
