// node:assert — assertion surface (real AssertionError throws + deep compare).
import { describe, test, expect } from "rts:test";
import {
    ok,
    equal,
    strictEqual,
    notStrictEqual,
    deepStrictEqual,
    notDeepStrictEqual,
    throws,
    doesNotThrow,
    ifError,
    fail,
} from "node:assert";

// Each assertion throws AssertionError on failure; inline try/catch captures
// pass (no throw) vs fail (throw) directly — no arrow reification.

// --- pass paths -------------------------------------------------------------
let okPass = false; try { ok(1); okPass = true; } catch (e) {}
let eqPass = false; try { equal(1, "1"); eqPass = true; } catch (e) {} // loose
let seqPass = false; try { strictEqual(5, 5); seqPass = true; } catch (e) {}
let neqPass = false; try { notStrictEqual(5, 6); neqPass = true; } catch (e) {}
let deepPass = false; try { deepStrictEqual([1, 2, 3], [1, 2, 3]); deepPass = true; } catch (e) {}
let deepObjPass = false; try { deepStrictEqual({ a: 1, b: 2 }, { a: 1, b: 2 }); deepObjPass = true; } catch (e) {}
let ifErrorPass = false; try { ifError(null); ifErrorPass = true; } catch (e) {}
let notDeepPass = false; try { notDeepStrictEqual([1, 2], [1, 3]); notDeepPass = true; } catch (e) {}

// --- fail paths (SHOULD throw) ----------------------------------------------
let okFail = false; try { ok(0); } catch (e) { okFail = true; }
let seqFail = false; try { strictEqual(5, 6); } catch (e) { seqFail = true; }
let seqTypeFail = false; try { strictEqual(5, "5"); } catch (e) { seqTypeFail = true; } // === type-strict
let deepFail = false; try { deepStrictEqual([1, 2], [1, 2, 3]); } catch (e) { deepFail = true; }
let deepObjFail = false; try { deepStrictEqual({ a: 1 }, { a: 2 }); } catch (e) { deepObjFail = true; }
let ifErrorFail = false; try { ifError("some error"); } catch (e) { ifErrorFail = true; }
let failAlways = false; try { fail(); } catch (e) { failAlways = true; }

// --- throws / doesNotThrow (fn arg) -----------------------------------------
let throwsPass = false; try { throws(() => { throw new Error("boom"); }); throwsPass = true; } catch (e) {}
let throwsFail = false; try { throws(() => { let x = 1; }); } catch (e) { throwsFail = true; }
let dntPass = false; try { doesNotThrow(() => { let x = 1; }); dntPass = true; } catch (e) {}
let dntFail = false; try { doesNotThrow(() => { throw new Error("x"); }); } catch (e) { dntFail = true; }

describe("node:assert", () => {
    test("ok pass", () => expect(okPass).toBe(true));
    test("equal loose pass", () => expect(eqPass).toBe(true));
    test("strictEqual pass", () => expect(seqPass).toBe(true));
    test("notStrictEqual pass", () => expect(neqPass).toBe(true));
    test("deepStrictEqual array pass", () => expect(deepPass).toBe(true));
    test("deepStrictEqual object pass", () => expect(deepObjPass).toBe(true));
    test("ifError null pass", () => expect(ifErrorPass).toBe(true));
    test("notDeepStrictEqual pass", () => expect(notDeepPass).toBe(true));
    test("ok(0) fails", () => expect(okFail).toBe(true));
    test("strictEqual mismatch fails", () => expect(seqFail).toBe(true));
    test("strictEqual type-strict fails", () => expect(seqTypeFail).toBe(true));
    test("deepStrictEqual len mismatch fails", () => expect(deepFail).toBe(true));
    test("deepStrictEqual obj mismatch fails", () => expect(deepObjFail).toBe(true));
    test("ifError(err) fails", () => expect(ifErrorFail).toBe(true));
    test("fail() always throws", () => expect(failAlways).toBe(true));
    test("throws(throwing) pass", () => expect(throwsPass).toBe(true));
    test("throws(no-throw) fails", () => expect(throwsFail).toBe(true));
    test("doesNotThrow(no-throw) pass", () => expect(dntPass).toBe(true));
    test("doesNotThrow(throwing) fails", () => expect(dntFail).toBe(true));
});
