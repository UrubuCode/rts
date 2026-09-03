// node:tty — terminal detection.
import { describe, test, expect } from "rts:test";
import { isatty, WriteStream } from "node:tty";

// PROVA (Node real v20.19.5): node -e "const t=require('node:tty');
// console.log(Object.keys(t))" -> [ 'isatty', 'ReadStream', 'WriteStream' ]
// getColorDepth/hasColors NUNCA foram exports de topo — vivem em
// tty.WriteStream.prototype, e so sao alcancaveis atraves de uma instancia,
// tal como o Node real os expoe.

// Under the test harness stdio is piped (not a TTY), so fd 1 is not a terminal.
const tty1 = isatty(1);
const ttyBogus = isatty(999); // non-std fd → false, never throws
const tty1IsBool = tty1 === true || tty1 === false;

const stream = new WriteStream(1);
const getColorDepthIsMethod = typeof stream.getColorDepth === "function";
const hasColorsIsMethod = typeof stream.hasColors === "function";

const depth = stream.getColorDepth();
// depth is one of Node's bit values.
const depthOk = depth === 1 || depth === 4 || depth === 8 || depth === 24;

// hasColors is consistent with the depth: 2**depth colors available.
const hc2 = stream.hasColors(2); // 2 colors always available (>= monochrome)
const hc16m = stream.hasColors(16777216);
const hc16mExpected = depth >= 24;

describe("node:tty", () => {
    test("isatty returns boolean", () => expect(tty1IsBool).toBe(true));
    test("isatty bogus fd false, does not throw", () => expect(ttyBogus).toBe(false));
    test("getColorDepth/hasColors live on WriteStream.prototype", () => {
        expect(getColorDepthIsMethod).toBe(true);
        expect(hasColorsIsMethod).toBe(true);
    });
    test("getColorDepth valid bits", () => expect(depthOk).toBe(true));
    test("hasColors(2) true", () => expect(hc2).toBe(true));
    test("hasColors(16M) matches depth", () => expect(hc16m).toBe(hc16mExpected));
});
