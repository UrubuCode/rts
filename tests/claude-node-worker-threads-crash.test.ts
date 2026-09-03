// node:worker_threads — an uncaught exception ON A WORKER'S OWN THREAD kills
// the WHOLE PROCESS, main thread included. Isolated here so
// `claude-node-worker-threads.test.ts` stays measurable.
//
// This directly contradicts the module's own doc, twice over:
//   - "nothing about a worker can corrupt the thread that made it" (the
//     module's "What a worker is here" section) — an uncaught throw does not
//     corrupt the parent thread, it takes the entire process down with it,
//     which is strictly worse.
//   - The documented delivery contract is that the worker's thread "queues
//     native data, and the parent turns that into '`message`'/'`error`'/
//     '`exit`' on its own thread" — `registry::start`'s spawned closure DOES
//     wrap the evaluator call and unconditionally deposits `Exited(0)`
//     afterward, so the INTENT is clearly for a failure to become an
//     `'error'` event. In practice the process dies before that line is ever
//     reached: `evaluator(&source)` itself does not return on an uncaught
//     JS exception — whatever this engine's top-level uncaught-exception
//     handler is (the same one that prints `rts: uncaught exception (tag 1):
//     ...` for an ordinary top-level script) fires from inside the WORKER's
//     thread and appears to end the process outright, never returning
//     control to `registry::start`'s closure to deposit `Failed`/`Exited`.
//
// Two independent triggers are given below, each minimal:
//
// 1. `require(...)` inside an `eval: true` worker source. CLAUDE.md states,
//    as a repository-wide invariant, that "`require`, `module`, `exports`...
//    are bound in every module that mentions them" — true for an ordinary
//    top-level script or import, but NOT inside a worker's evaluated source:
//    `require` there is simply unbound, so calling it is an ordinary
//    `ReferenceError`, and that ReferenceError is what tears down the
//    process. (Every worker source in the sibling file therefore uses
//    `import`, which DOES work inside a worker — this is specifically about
//    CommonJS's `require` being absent there.)
// 2. A plain, deliberate `throw new Error(...)` with no surrounding
//    try/catch — the simplest possible uncaught exception, with nothing
//    `require`-specific about it, to show the crash is about ANY uncaught
//    throw on a worker's thread and not particular to the missing global.
//
// Both were reproduced identically via `rts run` on a two-line script
// outside the test harness before being written here, so this is not an
// artifact of how `rts:test` drives a file.
import { describe, test, expect } from "rts:test";
import { Worker } from "node:worker_threads";
import { time } from "rts";

describe("node:worker_threads — an uncaught throw on a worker's thread", () => {
    test("require() inside eval:true source does not crash the process", () => {
        const w = new Worker("require('node:worker_threads');", { eval: true });
        let errorFired = false;
        w.on("error", () => {
            errorFired = true;
        });
        // THE KILLING LINE. The process dies here, inside the worker's own
        // thread, before this sleep_ms call even returns — nothing after it
        // in this file or process runs.
        time.sleep_ms(200);
        expect(errorFired).toBe(true);
    });

    test("a plain throw with no try/catch does not crash the process", () => {
        const w = new Worker("throw new Error('boom from worker');", { eval: true });
        let errorFired = false;
        w.on("error", () => {
            errorFired = true;
        });
        time.sleep_ms(200);
        expect(errorFired).toBe(true);
    });
});
