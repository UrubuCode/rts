// A `switch` label is `===` with no coercion, and it was the one comparison in
// the emitter that reached none of the settlements `emit_binary_inner` has:
// `switch.rs` called `strict_equals_proof`, which consulted `proven_binary` and
// nothing else. So `case null:` was a full crossing and `switch (typeof x)`
// built a string and compared its text at every label.
//
// The assertions are about VALUES. Every one was checked against node first.
import { describe, test, expect } from "rts:test";

describe("a switch label reaches the same settlements an operator does", () => {
  test("case null and case undefined are not each other", () => {
    function kind(v: any): string {
      switch (v) {
        case null:
          return "null";
        case undefined:
          return "undef";
        default:
          return "other";
      }
    }
    expect(kind(null)).toBe("null");
    expect(kind(undefined)).toBe("undef");
    // A switch label is STRICT, so nothing coerces into either.
    for (const near of [0, -0, "", "null", false, NaN, [], {}] as any[]) {
      expect(kind(near)).toBe("other");
    }
  });

  test("switch (typeof x) over all nine answers", () => {
    function kind(v: any): string {
      switch (typeof v) {
        case "number":
          return "N";
        case "string":
          return "S";
        case "boolean":
          return "B";
        case "undefined":
          return "U";
        case "object":
          return "O";
        case "function":
          return "F";
        case "symbol":
          return "Y";
        case "bigint":
          return "G";
        default:
          return "?";
      }
    }
    expect(kind(1)).toBe("N");
    expect(kind(NaN)).toBe("N");
    expect(kind("a")).toBe("S");
    expect(kind(true)).toBe("B");
    expect(kind(undefined)).toBe("U");
    expect(kind(null)).toBe("O");
    expect(kind({})).toBe("O");
    expect(kind([])).toBe("O");
    expect(kind(() => 1)).toBe("F");
    expect(kind(Symbol("s"))).toBe("Y");
    expect(kind(10n)).toBe("G");
    // A boxed number is an object, which a tag test reading the payload would
    // get wrong.
    expect(kind(new Number(1))).toBe("O");
  });

  test("a label that names none of the nine still answers", () => {
    function kind(v: any): string {
      switch (typeof v) {
        case "wrong":
          return "!";
        case "Number":
          return "!!";
        default:
          return "ok";
      }
    }
    expect(kind(1)).toBe("ok");
    expect(kind("a")).toBe("ok");
  });

  test("the discriminant is evaluated exactly ONCE", () => {
    // The whole reason `typeof` is emitted as an operand here rather than per
    // label: an `if` chain writes it once per test and a switch must not.
    const log: string[] = [];
    function watched(): any {
      log.push("read");
      return "s";
    }
    function kind(): string {
      switch (typeof watched()) {
        case "number":
          return "N";
        case "string":
          return "S";
        default:
          return "?";
      }
    }
    log.length = 0;
    expect(kind()).toBe("S");
    expect(log.length).toBe(1);
  });

  test("fall-through and default still work through the settled labels", () => {
    function group(v: any): string {
      let out = "";
      switch (typeof v) {
        case "number":
          out += "n";
        // falls through
        case "boolean":
          out += "b";
          break;
        case "undefined":
          out += "u";
          break;
        default:
          out += "d";
      }
      return out;
    }
    expect(group(1)).toBe("nb");
    expect(group(true)).toBe("b");
    expect(group(undefined)).toBe("u");
    expect(group("s")).toBe("d");
  });

  test("a switch over numbers and strings is unchanged", () => {
    function n(v: number): string {
      switch (v) {
        case 0:
          return "z";
        case 1:
          return "o";
        case NaN:
          return "nan";
        default:
          return "d";
      }
    }
    expect(n(0)).toBe("z");
    expect(n(1)).toBe("o");
    expect(n(NaN)).toBe("d");
    expect(n(2)).toBe("d");
    function s(v: string): string {
      switch (v) {
        case "a":
          return "A";
        case "b":
          return "B";
        default:
          return "D";
      }
    }
    expect(s("a")).toBe("A");
    expect(s("b")).toBe("B");
    expect(s("c")).toBe("D");
  });
});
