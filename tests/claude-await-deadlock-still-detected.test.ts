// The other side of #2893: with nothing outstanding, `await` on a promise
// nothing can settle STILL reports a deadlock.
//
// Making a listening server and a `setInterval` hold the program open changes
// what `entry::loops::pump_sources` answers, and `promise_await` reads that same
// answer to decide an `await` can never finish. So the fix had exactly one way
// to go wrong that no server fixture would catch: making the accounting
// permanently non-empty, after which the detector can never fire again and a
// genuinely stuck program hangs until something kills it.
//
// That detector is worth more than the error it prints. A program that parks on
// a promise with nothing left to settle it is a bug in the program, and the
// alternative to saying so is burning a core in silence.
//
// This file is the guard against weakening it, and it is deliberately the
// NEGATIVE case: no server, no timer, no socket, nothing registered at all.
//
// Caught rather than observed as an exit code, because the raise is a real
// catchable throw — `stall` builds the program's own error and the call site
// re-raises — so a `try`/`catch` can assert on it without the file having to
// fail to prove it.
import { describe, test, expect } from "rts:test";

let raised = "";

try {
  await new Promise(() => {});
  raised = "nothing was raised";
} catch (error: any) {
  raised = String(error && error.message ? error.message : error);
}

describe("the await deadlock detector still fires", () => {
  test("a promise nothing can settle is reported, not waited on", () => {
    expect(raised).not.toBe("nothing was raised");
    // On the message and not merely on "something threw": the thing being
    // guarded is this specific diagnosis, and any other error reaching here
    // would mean the park failed for an unrelated reason.
    expect(raised.indexOf("cannot settle") >= 0).toBe(true);
  });
});
