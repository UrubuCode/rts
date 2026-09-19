// `Number::toString` asks for the fewest digits that read back as the same
// double, and where two candidates are equally short AND equally close, for the
// one whose last digit is EVEN. 1658206780088562.25 is exactly a double, so
// `…562.2` and `…562.3` are a tie — node and bun print `.2`.
//
// This engine printed `.3`: it took its digits from Rust's `{:e}`, which breaks
// the tie upward. Nothing had asked. What found it was a unit test written to
// prove the replacement (`ryu`) EQUAL to `{:e}` over two hundred thousand
// doubles; it was the ruler that turned out to be wrong.
//
// Every expectation is what node answers. This file FAILS on a binary from
// before 2026-09-19, which is the point of it.
import { describe, test, expect } from "rts:test";

describe("a tie between two shortest spellings takes the even digit", () => {
  test("String, a template and JSON agree with node", () => {
    const tied = 1.6582067800885623e15;
    expect(String(tied)).toBe("1658206780088562.2");
    expect(`${tied}`).toBe("1658206780088562.2");
    expect(JSON.stringify([tied, -tied])).toBe("[1658206780088562.2,-1658206780088562.2]");
    expect(tied.toString()).toBe("1658206780088562.2");
  });

  test("and it still reads back as itself", () => {
    expect(Number("1658206780088562.2")).toBe(1.6582067800885623e15);
    expect(JSON.parse("1658206780088562.2")).toBe(1658206780088562.3);
  });
});
