import { describe, test, expect } from "rts:test";

// `in` walks to Object.prototype and finds inherited members as REAL slots
// (no hardcoded member list). isPrototypeOf is now a real slot too.

const o = { a: 1 };
const hasOwnA = "a" in o;
const hasToString = "toString" in o;          // inherited from Object.prototype
const hasHasOwn = "hasOwnProperty" in o;
const hasIsProtoOf = "isPrototypeOf" in o;     // the newly-slotted member
const hasValueOf = "valueOf" in o;
const hasNope = "nope" in o;

const arr = [1, 2, 3];
const arrPush = "push" in arr;                 // Array.prototype method
const arrToString = "toString" in arr;         // inherited Object.prototype
const arrLen = "length" in arr;

// isPrototypeOf still works as a call
const ipo = Object.prototype.isPrototypeOf(o);

describe("in operator over real proto slots", () => {
    test("own key", () => { expect(hasOwnA).toBe(true); });
    test("inherited toString", () => { expect(hasToString).toBe(true); });
    test("inherited hasOwnProperty", () => { expect(hasHasOwn).toBe(true); });
    test("inherited isPrototypeOf (slotted)", () => { expect(hasIsProtoOf).toBe(true); });
    test("inherited valueOf", () => { expect(hasValueOf).toBe(true); });
    test("absent key", () => { expect(hasNope).toBe(false); });
    test("array push", () => { expect(arrPush).toBe(true); });
    test("array inherited toString", () => { expect(arrToString).toBe(true); });
    test("array length", () => { expect(arrLen).toBe(true); });
    test("isPrototypeOf call works", () => { expect(ipo).toBe(true); });
});
