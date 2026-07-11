// node:crypto — randomInt range validation (Node conformance).
import { describe, test, expect } from "rts:test";
import { randomInt } from "node:crypto";
const inRange = randomInt(1, 5);
const inRangeOk = inRange >= 1 && inRange < 5;
let invMax = false;
try { randomInt(0); } catch (e) { invMax = true; }        // [0,0) invalid
let invRange = false;
try { randomInt(10, 5); } catch (e) { invRange = true; }  // max < min invalid
describe("node:crypto randomInt range", () => {
    test("in range", () => expect(inRangeOk).toBe(true));
    test("randomInt(0) throws", () => expect(invMax).toBe(true));
    test("max<min throws", () => expect(invRange).toBe(true));
});
