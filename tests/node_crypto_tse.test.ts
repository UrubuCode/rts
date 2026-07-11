// node:crypto — timingSafeEqual throws on length mismatch (Node conformance).
import { describe, test, expect } from "rts:test";
import { timingSafeEqual } from "node:crypto";
const eq = timingSafeEqual("hello", "hello");
const ne = timingSafeEqual("hello", "world");
let threw = false;
try { timingSafeEqual("ab", "abc"); } catch (e) { threw = true; }
describe("node:crypto timingSafeEqual", () => {
    test("equal", () => expect(eq).toBe(true));
    test("unequal same length", () => expect(ne).toBe(false));
    test("length mismatch throws", () => expect(threw).toBe(true));
});
