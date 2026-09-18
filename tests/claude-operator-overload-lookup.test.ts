import { describe, test, expect } from "rts:test";
import { operators } from "rts";

// The operator's Get has two paths in the runtime: a probe inside one borrow,
// for a data property with no proxy anywhere, and the generic Get for what only
// user code can answer — a getter, or a proxy trap. Each case below pins one
// side of that line, or the line itself.
//
// ORDER MATTERS in this file. The probe steps aside for the rest of the
// program once any Proxy exists, so every case that must exercise the probe
// comes before the one that makes a Proxy, which is last.

let hits = 0;

class V {
  x: number;
  constructor(x: number) {
    this.x = x;
  }
  [operators.add](other: any, reversed: boolean): any {
    hits++;
    return new V(this.x + (other instanceof V ? other.x : other));
  }
  valueOf(): number {
    return this.x;
  }
}

// The method comes from a getter: user code the probe cannot run in its borrow.
let getterRuns = 0;
let getterThis: any = null;
class G {
  n: number;
  constructor(n: number) {
    this.n = n;
  }
  get [operators.mul](): any {
    getterRuns++;
    getterThis = this;
    return function (this: G, k: number, reversed: boolean) {
      return this.n * k + (reversed ? 0.5 : 0);
    };
  }
}

// A getter that throws: the operator throws that, and never calls anything.
class Broken {
  get [operators.sub](): any {
    throw new Error("getter refused");
  }
  valueOf(): number {
    return 1;
  }
}

describe("the operator's Get sees what an ordinary Get sees", () => {
  test("a data method on the prototype, both sides", () => {
    hits = 0;
    const a: any = new V(2);
    const b: any = new V(5);
    expect((a + b).x).toBe(7);
    expect((a + 3).x).toBe(5);
    expect((3 + a).x).toBe(5);
    expect(hits).toBe(3);
  });

  test("a getter on the prototype runs, on the operand, each time", () => {
    getterRuns = 0;
    const g: any = new G(4);
    expect(g * 3).toBe(12);
    expect(getterThis === g).toBe(true);
    expect(2 * g).toBe(8.5);
    expect(getterRuns).toBe(2);
  });

  test("a getter that throws is the operator's throw", () => {
    let caught = "";
    try {
      const b: any = new Broken();
      b - 1;
    } catch (e: any) {
      caught = e.message;
    }
    expect(caught).toBe("getter refused");
  });

  test("a method replaced on the prototype is seen by the next use", () => {
    const a: any = new V(1);
    expect((a + 1).x).toBe(2);
    const original = (V.prototype as any)[operators.add];
    (V.prototype as any)[operators.add] = function (other: any) {
      return "replaced";
    };
    expect(a + 1).toBe("replaced");
    (V.prototype as any)[operators.add] = original;
    expect((a + 1).x).toBe(2);
  });

  test("a method removed from the prototype falls back to ToPrimitive", () => {
    const a: any = new V(1);
    const original = (V.prototype as any)[operators.add];
    delete (V.prototype as any)[operators.add];
    expect(a + 1).toBe(2);
    (V.prototype as any)[operators.add] = original;
    expect((a + 1).x).toBe(2);
  });

  test("an own symbol shadows the prototype's, on that object only", () => {
    const own: any = new V(1);
    const other: any = new V(1);
    own[operators.add] = function (x: any, reversed: boolean) {
      return reversed ? "own, reflected" : "own";
    };
    expect(own + 1).toBe("own");
    expect(1 + own).toBe("own, reflected");
    expect((other + 1).x).toBe(2);
  });

  test("an own NON-callable value shadows the prototype's method", () => {
    const plain: any = new V(6);
    plain[operators.add] = 42;
    // Declared nothing callable: the specification's `+`, through valueOf.
    expect(plain + 1).toBe(7);
  });

  test("a Proxy operand is asked through its trap, and last in this file", () => {
    hits = 0;
    const seen: any[] = [];
    const target = new V(10);
    const proxy: any = new Proxy(target, {
      get(t: any, key: any, receiver: any) {
        seen.push(key);
        return Reflect.get(t, key, receiver);
      },
    });
    const sum = proxy + 5;
    expect(sum.x).toBe(15);
    expect(seen[0] === operators.add).toBe(true);
    // With a Proxy now in existence, an ordinary operand still answers.
    expect((new V(1) + new V(2)).x).toBe(3);
    // A Proxy whose trap supplies the method the target never had.
    const empty: any = new Proxy({}, {
      get(t: any, key: any) {
        return key === operators.add ? () => "from the trap" : undefined;
      },
    });
    expect(empty + 1).toBe("from the trap");
    expect(1 + empty).toBe("from the trap");
    expect(hits).toBe(2);
  });
});
