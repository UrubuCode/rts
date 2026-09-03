// node:util — format / isDeepStrictEqual / string utilities.
import { describe, test, expect } from "rts:test";
import {
    format,
    isDeepStrictEqual,
    stripVTControlCharacters,
    toUSVString,
    getSystemErrorName,
    styleText,
} from "node:util";
import process from "node:process";

// --- format -----------------------------------------------------------------
const f1 = format("%s = %d", "count", 42);
const f2 = format("%s", "hello");
const f3 = format("%d + %d = %d", 1, 2, 3);
const f4 = format("100%% done");
const f5 = format("no specifiers", "extra", "args"); // extras appended
const f6 = format("just text");
const f7 = format("%i truncates", 3.9);

// --- isDeepStrictEqual ------------------------------------------------------
const de1 = isDeepStrictEqual([1, 2, 3], [1, 2, 3]);
const de2 = isDeepStrictEqual({ a: 1, b: 2 }, { a: 1, b: 2 });
const de3 = isDeepStrictEqual([1, 2], [1, 2, 3]);
const de4 = isDeepStrictEqual({ a: 1 }, { a: 2 });
const de5 = isDeepStrictEqual(5, 5);

// --- string utilities -------------------------------------------------------
const strip = stripVTControlCharacters("\x1b[31mred\x1b[39m text");
const usv = toUSVString("plain");

// getSystemErrorName's numbering is libuv's own, and libuv does NOT give
// ENOENT the same number on every OS: on POSIX it is `-ENOENT` (-2), but on
// Windows libuv has its own table and ENOENT is -4058.
// PROVA (Node real v20.19.5, win32): node -e "const u=require('node:util');
// console.log(JSON.stringify(u.getSystemErrorName(-2)),
// JSON.stringify(u.getSystemErrorName(-4058)))"
//   -> "Unknown system error -2" "ENOENT"
// The old assertion hardcoded -2 (the POSIX number) and failed on Windows,
// where rts (like Node) answers "Unknown system error -2" — correctly, since
// -2 names a DIFFERENT error there. Asking `process.platform` for the right
// number keeps this file honest on both, without inventing a new mechanism.
const enoentCode = process.platform === "win32" ? -4058 : -2;
const errN = getSystemErrorName(enoentCode);

// styleText always returns a string. Whether it wraps that string in an ANSI
// escape depends on colour support, which Node itself gates on TTY-ness and
// FORCE_COLOR/NO_COLOR — not on the color name alone.
// PROVA (Node real v20.19.5, no FORCE_COLOR, no TTY — this test harness's own
// shape of environment, stdio piped): node -e "const u=require('node:util');
// console.log(JSON.stringify(u.styleText('red','hi')))" -> "hi"
// (node -e with FORCE_COLOR=1 in the environment instead answers
// "\x1b[31mhi\x1b[39m" — so the escape is conditional, not absent from the API.)
// The old assertion expected the escaped form unconditionally, which is false
// for any runtime — Node included — under a harness with no TTY and no
// FORCE_COLOR, which is exactly this one.
const styled = styleText("red", "hi");
const styledIsString = typeof styled === "string";
const styledUnescapedHere = styled === "hi";

describe("node:util", () => {
    test("format %s %d", () => expect(f1).toBe("count = 42"));
    test("format %s", () => expect(f2).toBe("hello"));
    test("format multiple %d", () => expect(f3).toBe("1 + 2 = 3"));
    test("format %%", () => expect(f4).toBe("100% done"));
    test("format extra args", () => expect(f5).toBe("no specifiers extra args"));
    test("format plain", () => expect(f6).toBe("just text"));
    test("format %i truncates", () => expect(f7).toBe("3 truncates"));
    test("isDeepStrictEqual arrays", () => expect(de1).toBe(true));
    test("isDeepStrictEqual objects", () => expect(de2).toBe(true));
    test("isDeepStrictEqual array mismatch", () => expect(de3).toBe(false));
    test("isDeepStrictEqual object mismatch", () => expect(de4).toBe(false));
    test("isDeepStrictEqual primitives", () => expect(de5).toBe(true));
    test("stripVTControlCharacters", () => expect(strip).toBe("red text"));
    test("toUSVString identity", () => expect(usv).toBe("plain"));
    test("getSystemErrorName(ENOENT), by platform's own numbering", () => expect(errN).toBe("ENOENT"));
    test("styleText returns a string, unescaped with no TTY/FORCE_COLOR", () => {
        expect(styledIsString).toBe(true);
        expect(styledUnescapedHere).toBe(true);
    });
});
