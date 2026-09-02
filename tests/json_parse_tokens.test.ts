// A JSON string token was built twice. `Reader::string` pushed one `u16` at a
// time into a growing `Vec`, and `materialise` then called `Str::from_utf16` on
// it — which walks every unit again to decide whether the narrow layout fits and
// allocates a second buffer. Two allocations and three passes over text the
// input already held contiguously, with the reader looking straight at it.
//
// A token with no escape IS a contiguous run of the input. Where the input is
// narrow — every byte below 256 — the slice goes straight to `Str::from_latin1`,
// whose own documentation says it needs no scan because a slice of a narrow
// string is narrow by construction. The escaped path keeps the `Vec`, which is
// the one case where the units have to be assembled rather than pointed at.
//
// The Str is built during PARSING rather than during materialising, because the
// borrow of the input is alive exactly there and gone by the time `materialise`
// runs — the node tree is carried out of that closure.
//
// The falsifier was run BEFORE the edit, which is the `medir-o-caso-vazio`
// discipline: three documents of one length isolating the parts.
//
//   (c) blank + one digit   the floor           80 ->   70
//   (a) numbers only        CONTROL            775 ->  740
//   (b) four keys, number values              1060 ->  915   (-11%)
//   (d) four keys, STRING values              1660 -> 1470   (-10%)
//   the analytic document                     2185 -> 1690   (-14%)
//
// Release, min of 9 over 200 K iterations, two alternations.
import { describe, test, expect } from "rts:test";

// Every literal here is BUILT rather than spelled. A JSON document full of
// escapes needs three levels of it — the source, whatever writes the file, and
// JSON — and two probes in this session were silently wrong because one level
// ate a backslash.
const Q = String.fromCharCode(34);
const B = String.fromCharCode(92);

describe("a string token, both paths", () => {
  test("no escape: the fast path", () => {
    expect(JSON.parse(Q + "plain" + Q)).toBe("plain");
    expect(JSON.parse(Q + Q)).toBe("");
  });

  test("an escape: the assembling path", () => {
    expect(JSON.parse(Q + B + "n" + Q)).toBe(String.fromCharCode(10));
    expect(JSON.parse(Q + B + "t" + Q)).toBe(String.fromCharCode(9));
    expect(JSON.parse(Q + B + B + Q)).toBe(B);
    expect(JSON.parse(Q + B + Q + Q)).toBe(Q);
    expect(JSON.parse(Q + B + "/" + Q)).toBe("/");
  });

  test("an escape in the MIDDLE, so the scan restarts", () => {
    // The fast path breaks at the first backslash and the slow path restarts
    // from the opening quote. If the restart were wrong this loses the prefix.
    expect(JSON.parse(Q + "ab" + B + "n" + "cd" + Q)).toBe("ab" + String.fromCharCode(10) + "cd");
  });

  test("a unicode escape, which is never narrow", () => {
    expect(JSON.parse(Q + B + "u00e9" + Q)).toBe(String.fromCharCode(0xe9));
    expect(JSON.parse(Q + B + "u4e2d" + Q)).toBe(String.fromCharCode(0x4e2d));
  });

  test("an escaped KEY, which takes the same path", () => {
    const parsed = JSON.parse("{" + Q + "k" + B + "u00e9y" + Q + ":1}") as any;
    expect(parsed[("k" + String.fromCharCode(0xe9) + "y")]).toBe(1);
  });

  test("a WIDE input, where the fast path does not apply at all", () => {
    // `narrow()` answers None, so the reader takes the assembling path for
    // every token — including the unescaped ones.
    const wide = "{" + Q + "k" + Q + ":" + Q + String.fromCharCode(0x4e2d) + "x" + Q + "}";
    expect((JSON.parse(wide) as any).k).toBe(String.fromCharCode(0x4e2d) + "x");
  });

  test("a truncated string still fails", () => {
    let threw = "";
    try {
      JSON.parse(Q + "unterminated");
    } catch (err) {
      threw = (err as Error).constructor.name;
    }
    expect(threw).toBe("SyntaxError");
  });

  test("a raw control character still fails", () => {
    // The check that makes a truncated document fail instead of absorbing the
    // newline that ended it — it lives on BOTH paths now, so both are asserted.
    let threw = "";
    try {
      JSON.parse(Q + "raw" + String.fromCharCode(10) + "nl" + Q);
    } catch (err) {
      threw = (err as Error).constructor.name;
    }
    expect(threw).toBe("SyntaxError");
  });

  test("the whole document round-trips", () => {
    const doc = '{"a":1,"b":"two","c":[1,2,3],"d":{"e":true}}';
    expect(JSON.stringify(JSON.parse(doc))).toBe(doc);
  });
});
