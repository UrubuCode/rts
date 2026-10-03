// node:timers — re-export of the engine timer globals.
//
// This file asserted `typeof h === "number"` for all three handles, which pinned
// a DIVERGENCE: Node answers a `Timeout`/`Immediate` object. It is rewritten
// rather than deleted, to the shape Node 22 was measured to have — the handle is
// an object, its class is readable, and `clearTimeout`/`clearInterval` still take
// it. `tests/claude-timer-handle-object.test.ts` is the full contract; this one
// stays narrow, which is what `node:timers` re-exporting the globals needs it to
// be.
import { describe, test, expect } from "rts:test";
import { setTimeout, clearTimeout, setInterval, clearInterval, setImmediate } from "node:timers";

// Schedule a timer and immediately clear it — clearTimeout accepts the handle.
let fired = 0;
function bump() { fired = fired + 1; }
const h = setTimeout(bump, 1000);
clearTimeout(h);

const iv = setInterval(bump, 1000);
clearInterval(iv);

const im = setImmediate(bump);
const handlesOk = typeof h === "object" && typeof iv === "object" && typeof im === "object";

describe("node:timers", () => {
    test("setTimeout returns a Timeout", () => expect(h.constructor.name).toBe("Timeout"));
    test("setInterval returns a Timeout", () => expect(iv.constructor.name).toBe("Timeout"));
    test("setImmediate returns an Immediate", () => expect(im.constructor.name).toBe("Immediate"));
    test("all handles are objects", () => expect(handlesOk).toBe(true));
    test("a handle still coerces to its id", () => expect(typeof Number(h)).toBe("number"));
});
