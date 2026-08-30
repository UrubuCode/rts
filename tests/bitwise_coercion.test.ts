// The bitwise operators and `**` convert an object operand, which they did not.
//
// `a & b` is `ToInt32(ToNumber(a)) & ToInt32(ToNumber(b))` — the module header
// in `entry/bitwise.rs` said exactly that while the code read its operands
// through a pure numeric accessor that answers `NaN` for an object and cannot
// convert one. `ToInt32(NaN)` is zero, so every one of these answered as though
// the operand were zero, and `[7] & 15` was 0 where the language says 7.
//
// Three of these tests assert the CALL COUNT rather than the answer, because a
// conversion that is skipped and a conversion that returns the right value are
// the same answer and different programs.
import { describe, test, expect } from "rts:test";

describe("an object operand of a bitwise operator", () => {
  test("an ordinary array converts through its text", () => {
    // Not exotic: a one-element array inherits `valueOf` from Object.prototype,
    // which answers the array itself, so `toString` runs and produces "7".
    expect(([7] as any) & 15).toBe(7);
    expect(([7] as any) | 0).toBe(7);
    expect(([7] as any) ^ 1).toBe(6);
    expect(~([7] as any)).toBe(-8);
    expect(([7] as any) << 1).toBe(14);
    expect(([8] as any) >> 1).toBe(4);
    expect(([8] as any) >>> 1).toBe(4);
    expect(([7] as any) ** 2).toBe(49);
    expect(([] as any) & 1).toBe(0);
    // `[]` becomes `""` becomes 0, so this is 0 and not 1 — node was asked.
    expect(([] as any) ** 2).toBe(0);
  });

  test("a valueOf decides the value, on either side", () => {
    const three: any = { valueOf: () => 3 };
    expect(three & 1).toBe(1);
    expect(1 & three).toBe(1);
    expect(three | 0).toBe(3);
    expect(three ^ 1).toBe(2);
    expect(~three).toBe(-4);
    expect(three << 1).toBe(6);
    expect(three >> 1).toBe(1);
    expect(three >>> 1).toBe(1);
    expect(three ** 2).toBe(9);
    expect(2 ** three).toBe(8);
    const two: any = { valueOf: () => 2 };
    expect(three & two).toBe(2);
    expect(three ** two).toBe(9);
  });

  test("the compound assignment forms convert too", () => {
    const three: any = { valueOf: () => 3 };
    let x: any = 1;
    x &= three;
    expect(x).toBe(1);
    let y: any = 1;
    y <<= three;
    expect(y).toBe(8);
    let z: any = 2;
    z **= three;
    expect(z).toBe(8);
    let w: any = 8;
    w >>= three;
    expect(w).toBe(1);
  });

  test("the conversion RUNS, and exactly once per operand, in source order", () => {
    const log: string[] = [];
    function counting(tag: string, value: number): any {
      return {
        valueOf(): number {
          log.push(tag);
          return value;
        },
      };
    }
    const a = counting("a", 3);
    const b = counting("b", 2);
    expect(a & b).toBe(2);
    expect(log.join(",")).toBe("a,b");
    log.length = 0;
    expect(a ** b).toBe(9);
    expect(log.join(",")).toBe("a,b");
    log.length = 0;
    expect(~a).toBe(-4);
    expect(log.join(",")).toBe("a");
    log.length = 0;
    expect(a << b).toBe(12);
    expect(log.join(",")).toBe("a,b");
  });

  test("Symbol.toPrimitive wins over valueOf, with the NUMBER hint", () => {
    const hinted: any = {
      [Symbol.toPrimitive](hint: string): number {
        return hint === "number" ? 5 : 99;
      },
    };
    expect(hinted & 7).toBe(5);
    expect(hinted ** 2).toBe(25);
    expect(hinted << 1).toBe(10);
    expect(~hinted).toBe(-6);
  });

  test("a THROWING conversion stops the operator and the second operand", () => {
    const log: string[] = [];
    const boom: any = {
      valueOf(): number {
        log.push("boom");
        throw new Error("nope");
      },
    };
    const after: any = {
      valueOf(): number {
        log.push("after");
        return 2;
      },
    };
    for (const [name, run] of [
      ["and", () => boom & after],
      ["shl", () => boom << after],
      ["pow", () => boom ** after],
      ["not", () => ~boom],
    ] as [string, () => any][]) {
      log.length = 0;
      let caught = "";
      try {
        run();
      } catch (error) {
        caught = (error as Error).message;
      }
      expect(caught).toBe("nope");
      expect(name + ":" + log.join(",")).toBe(name + ":boom");
    }
  });

  test("the primitives that already worked still do", () => {
    expect(("3" as any) & 1).toBe(1);
    expect(("3" as any) ** 2).toBe(9);
    expect((true as any) & 1).toBe(1);
    expect((null as any) & 1).toBe(0);
    expect((undefined as any) & 1).toBe(0);
    expect((NaN as any) & 1).toBe(0);
    expect(2147483648 | 0).toBe(-2147483648);
    expect(4294967296 | 0).toBe(0);
    expect(1 << 32).toBe(1);
    expect(-1 >>> 0).toBe(4294967295);
  });

  test("bigints are unaffected by converting first", () => {
    expect((-1n & 3n) === 3n).toBe(true);
    expect((1n << 64n) === 18446744073709551616n).toBe(true);
    expect((2n ** 10n) === 1024n).toBe(true);
    expect(~1n === -2n).toBe(true);
    // An object whose conversion answers a bigint now reaches the bigint path
    // rather than becoming NaN, which is the one behaviour converting first
    // adds beyond fixing the numbers.
    const held: any = { valueOf: () => 3n };
    expect((held & 1n) === 1n).toBe(true);
  });
});
