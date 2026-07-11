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
const errN = getSystemErrorName(-2); // ENOENT
const styled = styleText("red", "hi");
const styledIsWrapped = styled.length > 2 && styled.indexOf("hi") >= 0;

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
    test("getSystemErrorName", () => expect(errN).toBe("ENOENT"));
    test("styleText wraps", () => expect(styledIsWrapped).toBe(true));
});
