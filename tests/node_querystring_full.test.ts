// node:querystring — full-surface parity test (native codec + object I/O).
import { describe, test, expect } from "rts:test";
import {
    parse,
    stringify,
    escape,
    unescape,
    encode,
    decode,
} from "node:querystring";

// --- escape / unescape ------------------------------------------------------
const esc1 = escape("hello world");
const esc2 = escape("a=b&c");
const unesc1 = unescape("a%20b");
const unesc2 = unescape("%E2%9C%93"); // ✓ (UTF-8 round-trip)

// --- parse ------------------------------------------------------------------
const p1 = parse("a=1&b=2");
const p1a = p1.a;
const p1b = p1.b;
const p2 = parse("a=1&a=2&a=3"); // repeated key → array
const p2arr = p2.a;
const p2len = p2arr.length;
const p2first = p2arr[0];
const p2last = p2arr[2];
const p3 = parse("x=b+c"); // '+' → space
const p3x = p3.x;
const p4 = parse(""); // empty → {}
const p4keys = Object.keys(p4).length;
const p5 = parse("k"); // no '=' → value ''
const p5k = p5.k;
const p6 = parse("a:1;b:2", ";", ":"); // custom sep/eq
const p6a = p6.a;
const p6b = p6.b;

// --- stringify --------------------------------------------------------------
const s1 = stringify({ a: "1", b: "2" });
const s2 = stringify({ a: ["1", "2"] }); // array → repeated
const s3 = stringify({ q: "b c" }); // space → %20
const s4 = stringify({ n: 42, ok: true }); // number/bool coercion
const s5 = stringify({}); // empty → ''
const s6 = stringify({ a: "1", b: "2" }, ";", ":"); // custom sep/eq

// --- roundtrip + aliases ----------------------------------------------------
const round = stringify(parse("x=1&y=hello%20world"));
const encEq = encode({ a: "1" }) === stringify({ a: "1" });
const decEq = (decode("a=1").a) === "1";

describe("node:querystring full surface", () => {
    test("escape space", () => expect(esc1).toBe("hello%20world"));
    test("escape reserved", () => expect(esc2).toBe("a%3Db%26c"));
    test("unescape %20", () => expect(unesc1).toBe("a b"));
    test("unescape utf8", () => expect(unesc2).toBe("✓"));
    test("parse simple a", () => expect(p1a).toBe("1"));
    test("parse simple b", () => expect(p1b).toBe("2"));
    test("parse repeated → array len", () => expect(p2len).toBe(3));
    test("parse repeated first", () => expect(p2first).toBe("1"));
    test("parse repeated last", () => expect(p2last).toBe("3"));
    test("parse plus to space", () => expect(p3x).toBe("b c"));
    test("parse empty → {}", () => expect(p4keys).toBe(0));
    test("parse no eq → empty val", () => expect(p5k).toBe(""));
    test("parse custom sep/eq a", () => expect(p6a).toBe("1"));
    test("parse custom sep/eq b", () => expect(p6b).toBe("2"));
    test("stringify simple", () => expect(s1).toBe("a=1&b=2"));
    test("stringify array", () => expect(s2).toBe("a=1&a=2"));
    test("stringify space", () => expect(s3).toBe("q=b%20c"));
    test("stringify number+bool", () => expect(s4).toBe("n=42&ok=true"));
    test("stringify empty", () => expect(s5).toBe(""));
    test("stringify custom sep/eq", () => expect(s6).toBe("a:1;b:2"));
    test("roundtrip", () => expect(round).toBe("x=1&y=hello%20world"));
    test("encode === stringify", () => expect(encEq).toBe(true));
    test("decode === parse", () => expect(decEq).toBe(true));
});
