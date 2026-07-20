// The integer JUMP TABLE for `switch` (CRANELIFT_IMPLEMENTATION.md step 2) is
// only sound because of an exactness guard: the discriminant is narrowed to an
// i64 key, converted BACK to f64, and the table is entered only when the
// round-trip is bit-exact. Without it, `switch (1.5)` would truncate to 1 and
// wrongly enter `case 1:`.
//
// These cases pin the guard's behaviour against real JS semantics (verified
// against Node). They are the ones a naive truncate-and-jump gets WRONG.
import { describe, test, expect } from "rts:test";

function pick(x: number): string {
  switch (x) {
    case 0: return "zero";
    case 1: return "one";
    case 2: return "two";
    case 10: return "ten";
    case -3: return "neg3";
    default: return "other";
  }
}

// Fall-through must survive the table dispatch: the table only chooses the
// ENTRY block; bodies still fall into the next one until a `break`.
function fall(x: number): string {
  let s = "";
  switch (x) {
    case 1: s = s + "a";
    case 2: s = s + "b"; break;
    case 3: s = s + "c";
    default: s = s + "d";
  }
  return s;
}

// JS dispatches to the FIRST matching case; a duplicate key must not win.
function dup(x: number): string {
  switch (x) {
    case 1: return "first";
    case 1: return "second";
    default: return "def";
  }
}

const exact = `${pick(0)} ${pick(1)} ${pick(2)} ${pick(10)} ${pick(-3)} ${pick(7)}`;
const fractional = pick(1.5);
const negZero = pick(-0);
const nan = pick(0 / 0);
const posInf = pick(1 / 0);
const negInf = pick(-1 / 0);
const huge = pick(1e300);
const falls = `${fall(1)} ${fall(2)} ${fall(3)} ${fall(9)}`;
const duplicate = dup(1);

describe("switch integer jump table — edges", () => {
  test("exact integer keys, including a negative one", () => {
    expect(exact).toBe("zero one two ten neg3 other");
  });

  test("a fractional discriminant must NOT truncate into case 1", () => {
    expect(fractional).toBe("other");
  });

  test("-0 enters case 0 (JS: -0 === 0)", () => {
    expect(negZero).toBe("zero");
  });

  test("NaN matches nothing (NaN === NaN is false)", () => {
    expect(nan).toBe("other");
  });

  test("infinities take the default", () => {
    expect(posInf).toBe("other");
    expect(negInf).toBe("other");
  });

  test("a value beyond i64 range takes the default", () => {
    expect(huge).toBe("other");
  });

  test("fall-through still works through the table", () => {
    expect(falls).toBe("ab b cd d");
  });

  test("a duplicate case value keeps the first body", () => {
    expect(duplicate).toBe("first");
  });
});
