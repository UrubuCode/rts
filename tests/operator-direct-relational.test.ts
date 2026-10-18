import { describe, test, expect } from "rts:test";
import { operators } from "rts";

// `a < b` (and `<= > >= ==`) between two proven instances of one operator class is
// emitted as `!!a[op](b, false)` (emit/operator_direct.rs): the runtime answers
// ToBoolean of the method's result, so a method returning a non-boolean still yields a
// real boolean on both paths. The parameter cases are the runtime path beside them.

class Num {
  x: number;
  constructor(x: number) {
    this.x = x;
  }
  [operators.lt](o: any, r: boolean): any {
    return this.x < o.x;
  }
  [operators.le](o: any, r: boolean): any {
    return this.x <= o.x;
  }
  [operators.gt](o: any, r: boolean): any {
    return this.x > o.x;
  }
  [operators.ge](o: any, r: boolean): any {
    return this.x >= o.x;
  }
  [operators.eq](o: any, r: boolean): any {
    return this.x == o.x;
  }
}
// Every method answers a NON-boolean, so only ToBoolean makes the result a boolean.
class Raw {
  x: number;
  constructor(x: number) {
    this.x = x;
  }
  [operators.lt](o: any, r: boolean): any {
    return o.x - this.x;
  }
  [operators.le](o: any, r: boolean): any {
    return this.x === o.x ? "" : "yes";
  }
  [operators.gt](o: any, r: boolean): any {
    return this.x > o.x ? { a: 1 } : 0;
  }
  [operators.ge](o: any, r: boolean): any {
    return this.x >= o.x ? NaN : "0";
  }
  [operators.eq](o: any, r: boolean): any {
    return this.x - o.x;
  }
}
class NoLt {
  x = 1;
}
class Boom {
  [operators.lt](): any {
    throw new Error("boom");
  }
}

const lt = (a: any, b: any): any => a < b;
const le = (a: any, b: any): any => a <= b;
const gt = (a: any, b: any): any => a > b;
const ge = (a: any, b: any): any => a >= b;
const eq = (a: any, b: any): any => a == b;

describe("direct dispatch of < <= > >= == between two instances of one operator class", () => {
  for (const [a, b] of [
    [1, 2],
    [2, 1],
    [2, 2],
  ]) {
    test(`Num ${a},${b} direct equals runtime`, () => {
      expect(new Num(a) < new Num(b)).toBe(lt(new Num(a), new Num(b)));
      expect(new Num(a) <= new Num(b)).toBe(le(new Num(a), new Num(b)));
      expect(new Num(a) > new Num(b)).toBe(gt(new Num(a), new Num(b)));
      expect(new Num(a) >= new Num(b)).toBe(ge(new Num(a), new Num(b)));
      expect(new Num(a) == new Num(b)).toBe(eq(new Num(a), new Num(b)));
    });
    test(`Raw ${a},${b} non-boolean results are ToBoolean on both paths`, () => {
      const d: any[] = [
        new Raw(a) < new Raw(b),
        new Raw(a) <= new Raw(b),
        new Raw(a) > new Raw(b),
        new Raw(a) >= new Raw(b),
        new Raw(a) == new Raw(b),
      ];
      const r: any[] = [
        lt(new Raw(a), new Raw(b)),
        le(new Raw(a), new Raw(b)),
        gt(new Raw(a), new Raw(b)),
        ge(new Raw(a), new Raw(b)),
        eq(new Raw(a), new Raw(b)),
      ];
      for (let i = 0; i < 5; i++) {
        expect(typeof d[i]).toBe("boolean");
        expect(d[i]).toBe(r[i]);
      }
    });
  }
  test("=== is not overloaded and primitives fall back", () => {
    const p = new Num(1);
    const q = new Num(1);
    expect(p === q).toBe(false);
    expect(1 < 2 && 2 <= 2 && 3 > 1 && 2 == 2 && 2 === 2).toBe(true);
  });
  test("a class without lt falls back", () => {
    const a = new NoLt();
    const b = new NoLt();
    expect(a < b).toBe(lt(a, b));
  });
  test("a throwing method propagates", () => {
    let direct = "";
    let runtime = "";
    try {
      new Boom() < new Boom();
    } catch (e) {
      direct = (e as Error).message;
    }
    try {
      lt(new Boom(), new Boom());
    } catch (e) {
      runtime = (e as Error).message;
    }
    expect(direct).toBe("boom");
    expect(runtime).toBe("boom");
  });
});
