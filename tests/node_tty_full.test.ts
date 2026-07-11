// node:tty — terminal detection.
import { describe, test, expect } from "rts:test";
import { isatty, getColorDepth, hasColors } from "node:tty";

// Under the test harness stdio is piped (not a TTY), so fd 1 is not a terminal.
const tty1 = isatty(1);
const ttyBogus = isatty(999); // non-std fd → false
const tty1IsBool = tty1 === true || tty1 === false;

const depth = getColorDepth(1);
// depth is one of Node's bit values.
const depthOk = depth === 1 || depth === 4 || depth === 8 || depth === 24;

// hasColors is consistent with the depth: 2**depth colors available.
const hc2 = hasColors(2, 1); // 2 colors always available (>= monochrome)
const hc16m = hasColors(16777216, 1);
const hc16mExpected = depth >= 24;

describe("node:tty", () => {
    test("isatty returns boolean", () => expect(tty1IsBool).toBe(true));
    test("isatty bogus fd false", () => expect(ttyBogus).toBe(false));
    test("getColorDepth valid bits", () => expect(depthOk).toBe(true));
    test("hasColors(2) true", () => expect(hc2).toBe(true));
    test("hasColors(16M) matches depth", () => expect(hc16m).toBe(hc16mExpected));
});
