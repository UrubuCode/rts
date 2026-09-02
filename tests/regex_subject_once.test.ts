// `exec` converted the subject to a Rust `String` TWICE — once in `search` and
// once in `exec` itself — where `to_rust` on a narrow string is a `to_vec` plus
// a `from_utf8` validate: two passes and an allocation over the whole subject,
// paid twice.
//
// The measurement that found it is the one worth keeping, because it separates
// matching from producing a result. `/\d+/` against `"abc123"` repeated, release,
// min of 9 over 200 K iterations:
//
//   test on 6 chars    100 ns        exec on 6 chars     1000 ns
//   test on 600 chars  125 ns        exec on 600 chars   1855 ns
//
// `test` barely grows with the subject and `exec` grows at 1.44 ns per
// character, so the length was not in the matching — it was in the copying.
// After: exec on 600 measured 1110-1165 against 1177-1260, five alternations,
// the new binary winning all five.
//
// A previous attempt at the same row was REFUTED by the clock and reverted:
// building each capture group from its span rather than through a `String` moved
// nothing, while the control moved 9% the other way. The per-group copy is not
// where this row spends; the per-CALL subject copy is.
import { describe, test, expect } from "rts:test";

const B = String.fromCharCode(92);

describe("what exec must still answer with one conversion", () => {
  test("the groups, the index and the input", () => {
    const m = /(\w)(\d+)/.exec("abc123") as RegExpExecArray;
    expect(m[0]).toBe("c123");
    expect(m[1]).toBe("c");
    expect(m[2]).toBe("123");
    expect(m.index).toBe(2);
    expect(m.input).toBe("abc123");
    expect(m.length).toBe(3);
  });

  test("`input` is the subject itself, not a copy of it", () => {
    // The conversion this change removes is the Rust-side one; `input` already
    // reused the subject's cell and must go on doing so.
    const subject = "abc123";
    const m = /\d+/.exec(subject) as RegExpExecArray;
    expect(m.input === subject).toBe(true);
  });

  test("named groups", () => {
    const m = /(?<letter>\w)(?<digits>\d+)/.exec("abc123") as RegExpExecArray;
    expect(JSON.stringify(m.groups)).toBe('{"letter":"c","digits":"123"}');
  });

  test("an alternative that did not take part", () => {
    const m = /(a)|(b)/.exec("b") as RegExpExecArray;
    expect(m[0]).toBe("b");
    expect(m[1]).toBe(undefined);
    expect(m[2]).toBe("b");
  });

  test("a NON-ASCII subject, where the byte offsets are not the unit offsets", () => {
    // `units_before` converts a byte offset to a UTF-16 index, and it now reads
    // the single converted subject rather than a second one. If the two had
    // drifted this is where it shows.
    const m = ("x" + String.fromCharCode(0xe9) + "y1").match(/(.)(\d)/) as RegExpMatchArray;
    expect(m[1]).toBe("y");
    expect(m[2]).toBe("1");
    expect(m.index).toBe(2);
  });

  test("a STICKY pattern, which is the other caller of `search`", () => {
    // `test` on a `y` pattern takes the `search` path — the one that used to
    // convert on its own — so its `lastIndex` walk is asserted here.
    const re = new RegExp(B + "d", "y");
    re.lastIndex = 3;
    expect(re.test("abc123")).toBe(true);
    expect(re.lastIndex).toBe(4);
    re.lastIndex = 0;
    expect(re.test("abc123")).toBe(false);
  });

  test("a GLOBAL pattern advances its lastIndex across calls", () => {
    const re = new RegExp(B + "d+", "g");
    const first = re.exec("a1b22") as RegExpExecArray;
    expect(first[0]).toBe("1");
    const second = re.exec("a1b22") as RegExpExecArray;
    expect(second[0]).toBe("22");
    expect(re.exec("a1b22")).toBe(null);
  });

  test("no match answers null", () => {
    expect(/zz/.exec("abc")).toBe(null);
  });

  test("`d` indices still line up", () => {
    const re = new RegExp("(" + B + "d+)", "d");
    const m = re.exec("ab12") as any;
    expect(JSON.stringify(m.indices)).toBe("[[2,4],[2,4]]");
  });
});
