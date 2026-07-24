import { describe, test, expect } from "rts:test";

// A Registry-class instance (Date) resolves getPrototypeOf to a real object
// (Date.prototype → Object.prototype), via a by-class routing recorded at the
// ctor (NOT a fixed proto link) — so Proxy (trap-routed) is unaffected.

const d = new Date(0);
const dp = Object.getPrototypeOf(d);
const dpNotNull = dp !== null;
const reachesObject = Object.getPrototypeOf(dp) !== null;
const stillInstanceof = d instanceof Date;
const timeIsZero = d.getTime() === 0;
const objProtoOfD = Object.prototype.isPrototypeOf(d);

describe("registry instance prototype (Date)", () => {
    test("getPrototypeOf(new Date()) is not null", () => { expect(dpNotNull).toBe(true); });
    test("instance chain reaches Object.prototype", () => { expect(reachesObject).toBe(true); });
    test("instanceof Date still holds", () => { expect(stillInstanceof).toBe(true); });
    test("method dispatch unaffected", () => { expect(timeIsZero).toBe(true); });
    test("Object.prototype.isPrototypeOf(date)", () => { expect(objProtoOfD).toBe(true); });
});
