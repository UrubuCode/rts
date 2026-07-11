// node:punycode — full-surface parity test (RFC 3492 + IDN + ucs2).
import { describe, test, expect } from "rts:test";
import {
    decode,
    encode,
    toASCII,
    toUnicode,
    ucs2,
    version,
} from "node:punycode";

// --- core encode/decode -----------------------------------------------------
const d1 = decode("maana-pta");
const d2 = decode("--dqo34k");
const e1 = encode("mañana");
const e2 = encode("☃-⌘");
const rt1 = decode(encode("mañana")); // round-trip
const rt2 = decode(encode("Hello-World"));

// --- toASCII / toUnicode ----------------------------------------------------
const a1 = toASCII("mañana.com");
const a2 = toASCII("☃-⌘.com");
const a3 = toASCII("example.com"); // all-ASCII no-op
const u1 = toUnicode("xn--maana-pta.com");
const u2 = toUnicode("xn----dqo34k.com");
const u3 = toUnicode("example.com"); // no-op
const u4 = toUnicode("XN--maana-pta.com"); // case-insensitive prefix
const a4 = toASCII("test@mañana.com"); // userinfo passthrough

// --- ucs2 (object of callable function values) ------------------------------
const uc1 = ucs2.decode("abc");
const uc1len = uc1.length;
const uc1a = uc1[0];
const uc2 = ucs2.decode("𝌆"); // astral → one codepoint
const uc2len = uc2.length;
const uc2v = uc2[0];
const uc3 = ucs2.encode([0x61, 0x62, 0x63]);
const uc4 = ucs2.encode([0x1d306]); // → astral char
const isArr = Array.isArray(ucs2.decode("abc"));

const verVal = version;
const ver = typeof verVal;

describe("node:punycode full surface", () => {
    test("decode mañana", () => expect(d1).toBe("mañana"));
    test("decode snowman", () => expect(d2).toBe("☃-⌘"));
    test("encode mañana", () => expect(e1).toBe("maana-pta"));
    test("encode snowman", () => expect(e2).toBe("--dqo34k"));
    test("roundtrip accented", () => expect(rt1).toBe("mañana"));
    test("roundtrip ascii", () => expect(rt2).toBe("Hello-World"));
    test("toASCII mañana", () => expect(a1).toBe("xn--maana-pta.com"));
    test("toASCII snowman", () => expect(a2).toBe("xn----dqo34k.com"));
    test("toASCII noop", () => expect(a3).toBe("example.com"));
    test("toUnicode mañana", () => expect(u1).toBe("mañana.com"));
    test("toUnicode snowman", () => expect(u2).toBe("☃-⌘.com"));
    test("toUnicode noop", () => expect(u3).toBe("example.com"));
    test("toUnicode case-insensitive", () => expect(u4).toBe("mañana.com"));
    test("toASCII userinfo", () => expect(a4).toBe("test@xn--maana-pta.com"));
    test("ucs2.decode length", () => expect(uc1len).toBe(3));
    test("ucs2.decode value", () => expect(uc1a).toBe(0x61));
    test("ucs2.decode astral len", () => expect(uc2len).toBe(1));
    test("ucs2.decode astral value", () => expect(uc2v).toBe(0x1d306));
    test("ucs2.encode abc", () => expect(uc3).toBe("abc"));
    test("ucs2.encode astral", () => expect(uc4).toBe("𝌆"));
    test("ucs2.decode isArray", () => expect(isArr).toBe(true));
    test("version is string", () => expect(ver).toBe("string"));
});
