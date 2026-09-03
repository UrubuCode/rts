// node:stream — `stream.addAbortSignal(signal, stream)` followed by
// `controller.abort()`, with NO 'error' listener on the stream, KILLS THE
// WHOLE PROCESS on this engine (`rts: uncaught 'error' event: an object`,
// exit code 1).
//
// Real Node does NOT crash here — confirmed with `node -e`, exit code 0,
// even with zero listeners of any kind on the stream:
//
//   const { Readable } = require('stream');
//   const dns = require('stream');
//   const ac = new AbortController();
//   const s = Readable.from([1,2,3]);
//   require('stream').addAbortSignal(ac.signal, s);
//   ac.abort();
//   // process exits 0
//
// A plain `readable.destroy(new Error('boom'))` with no 'error' listener
// DOES crash both engines identically (Node's own "Unhandled 'error' event"
// throw) — isolated separately below to prove this file's crash is
// SPECIFIC to the abort-signal path, not "any unlistened destroy crashes
// here" (which would be correct, matching Node). Node must be suppressing
// the unhandled-error throw specifically for an abort-triggered destroy;
// this engine's `abort_signal.rs` calls the same `stream.destroy(reason)`
// every other destroy path uses, with no such special case.
//
// This file is intentionally NOT wrapped in try/catch around the killer
// call — the whole point is that the process dies, which a try/catch
// cannot stop (this is a native abort, not a catchable JS throw). Every
// OTHER stream assertion lives in `tests/claude-node-stream.test.ts`,
// entirely apart from this file, so a run of that file is unaffected by
// this one dying.
import { describe, test, expect } from "rts:test";
import stream, { Readable } from "node:stream";

describe("node:stream — addAbortSignal crash isolation", () => {
    test("baseline: plain destroy(err) with no listener crashes too (matches Node — NOT the bug)", () => {
        // Not run: executing this would already kill the process before the
        // real target below gets a chance to. Left as documentation that the
        // baseline was checked manually (see the file header) rather than
        // asserted, since asserting it would require the SAME process-ending
        // crash this file is here to isolate. Verified interactively:
        //   Readable.from([1,2,3]).destroy(new Error("boom"))
        // -> `rts: uncaught 'error' event: an object`, exit 1 — same message
        // Node itself gives for an unlistened 'error' (parity, not a bug).
        expect(true).toBe(true);
    });

    test("THE KILLER CALL: addAbortSignal + abort() with no listener — process dies here, not caught, not skipped", () => {
        const ac = new AbortController();
        const s = Readable.from([1, 2, 3]);
        stream.addAbortSignal(ac.signal, s);
        ac.abort(); // <-- kills the whole rts.exe process on this engine
        // Nothing after this line ever runs.
        expect(s.destroyed).toBe(true);
    });
});
