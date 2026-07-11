// node:timers — re-export of the engine timer globals.
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
const handlesOk = typeof h === "number" && typeof iv === "number" && typeof im === "number";

describe("node:timers", () => {
    test("setTimeout returns handle", () => expect(typeof h === "number").toBe(true));
    test("setInterval returns handle", () => expect(typeof iv === "number").toBe(true));
    test("setImmediate returns handle", () => expect(typeof im === "number").toBe(true));
    test("all handles numeric", () => expect(handlesOk).toBe(true));
});
