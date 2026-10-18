// A captured stack frame now carries WHERE in the source the function is, not
// only its name. The machine records a source position per instruction and the
// code map attributes an address back to one; `.stack` renders it as
// `at <name>:<position>`. Before this, a frame was `at <name>` alone — a name
// a reader could not locate in a minified bundle, which is exactly the case the
// position exists for.
//
// The position is a raw, opaque number (a byte offset into the source), so this
// pins the SHAPE — a name followed by `:` and digits — rather than any one
// value, which would break the day the source text moved by a character.

import { describe, test, expect } from "rts:test";

describe("a stack frame carries a source position", () => {
  test("a constructed Error names its frame with a position", () => {
    function throwing(): string {
      return String(new Error("here").stack);
    }
    const stack = throwing();
    // The frame is present at all.
    expect(stack.indexOf("at throwing") >= 0).toBe(true);
    // And it carries a raw position: `at throwing:<digits>`.
    expect(/at throwing:\d+/.test(stack)).toBe(true);
  });

  test("the position is a number a reader could act on, not zero-width", () => {
    function located(): string {
      const e = new Error("x");
      return String(e.stack);
    }
    const match = /at located:(\d+)/.exec(located());
    expect(match !== null).toBe(true);
    // A real offset into the source, not the "nothing said" sentinel.
    expect(Number(match![1]) > 0).toBe(true);
  });
});
