// node:process — nextTick schedules a callback (deferred firing verified via
// `run`: prints "sync" then "tick"). Here we assert it is callable and accepts a
// function without throwing.
import { describe, test, expect } from "rts:test";
import { nextTick } from "node:process";

function noop() {}
let called = false;
try { nextTick(noop); called = true; } catch (e) {}

describe("node:process nextTick", () => {
    test("nextTick accepts a callback", () => expect(called).toBe(true));
});
