// `JSON.stringify` classified every member TWICE and copied every string it
// wrote. `Shape::Text` carried an owned `Str` — a copy of the whole buffer —
// because a `Shape` is carried out of the `with_current` closure that made it,
// and nothing had asked whether it needed to be. It did not: the write touches
// no context, only the output buffer, so it can happen inside the borrow, which
// is what a member's KEY already did one screen below.
//
// And `plain` asked `shape_of` solely to see `Absent` — rule 8, so that a
// `toJSON` answering `undefined` cannot produce `{"drop":}` — after which
// `write` asked the identical question again. The decision is carried now
// rather than re-asked; the question is not removed.
//
// Measured, release, min of 9 over 200 K iterations:
//
//   stringify FOUR STRINGS            1260-1305 -> 980-1000   (-23%)
//   stringify FOUR NUMBERS  CONTROL   1025-1035 -> 1030-1040
//
// Every expectation here was checked against node first.
import { describe, test, expect } from "rts:test";

describe("what stringify must still answer", () => {
  const small = { a: 1, b: "two", c: [1, 2, 3], d: { e: true } };

  test("the shape the benchmark uses", () => {
    expect(JSON.stringify(small)).toBe('{"a":1,"b":"two","c":[1,2,3],"d":{"e":true}}');
  });

  test("an indented form, where the indent is itself classified", () => {
    expect(JSON.stringify(small, null, 2).length).toBe(88);
  });

  test("a member that is `undefined` is dropped, not written empty", () => {
    // Rule 8's question, the one the removed pre-test existed to ask.
    expect(JSON.stringify({ x: undefined, y: 1 })).toBe('{"y":1}');
  });

  test("a `toJSON` hook replaces the member", () => {
    expect(JSON.stringify({ t: { toJSON: () => "hooked" } })).toBe('{"t":"hooked"}');
  });

  test("a replacer LIST selects members, and reads its own entries as text", () => {
    // The second consumer of `Shape::Text` — it reads the list's elements to
    // learn which keys to keep, and now reads them from the cell.
    expect(JSON.stringify(small, ["a", "c"])).toBe('{"a":1,"c":[1,2,3]}');
  });

  test("an array writes `null` where an object drops", () => {
    expect(JSON.stringify([1, "s", null, undefined, () => 1])).toBe('[1,"s",null,null,null]');
  });

  test("infinities and NaN are null", () => {
    expect(JSON.stringify({ n: 1 / 0, m: NaN })).toBe('{"n":null,"m":null}');
  });

  test("a string is still escaped", () => {
    // Built rather than spelled: the literal needs three levels of escaping —
    // the source, the shell that wrote this file, and JSON itself — and the
    // first version of this test got the middle one wrong.
    const q = String.fromCharCode(34);
    const bs = String.fromCharCode(92);
    const raw = "quo" + q + "te" + String.fromCharCode(10) + String.fromCharCode(9);
    const want = "{" + q + "s" + q + ":" + q + "quo" + bs + q + "te" + bs + "n" + bs + "t" + q + "}";
    expect(JSON.stringify({ s: raw })).toBe(want);
  });

  test("a STRING indent, which the third consumer of `Shape::Text` reads", () => {
    expect(JSON.stringify({ a: 1 }, null, "--")).toBe('{\n--"a": 1\n}');
  });

  test("a bigint still refuses", () => {
    let threw = "";
    try {
      JSON.stringify({ b: BigInt(1) });
    } catch (err) {
      threw = (err as Error).constructor.name;
    }
    expect(threw).toBe("TypeError");
  });
});
