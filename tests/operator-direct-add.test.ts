import { describe, test, expect } from "rts:test";
import { operators } from "rts";

// `a + b` between two proven instances of one `[operators.add]` class is emitted as
// the direct call `a[add](b, false)` (emit/operator_direct.rs). Each case answers what
// the runtime's overload protocol answers, so the direct and runtime paths are
// pinned to one value; the parameter cases keep the runtime path honest beside them.

let calls = 0;
class Vec {
  x: number;
  constructor(x: number) {
    this.x = x;
  }
  [operators.add](o: any, reversed: boolean): any {
    calls++;
    return new Vec(this.x + o.x + (reversed ? 1000 : 0));
  }
}
class Other {
  x: number;
  constructor(x: number) {
    this.x = x;
  }
  [operators.add](o: any, reversed: boolean): any {
    return "Other:" + this.x + ":" + reversed;
  }
}
class Plain {
  x = 1;
}
class Boom {
  [operators.add](): any {
    throw new Error("boom");
  }
}

function viaParams(a: any, b: any): any {
  return a + b;
}

describe("direct dispatch of + between two instances of one operator class", () => {
  test("new C() + new C() equals the runtime path (parameters)", () => {
    calls = 0;
    const direct: any = new Vec(2) + new Vec(3);
    const runtime: any = viaParams(new Vec(2), new Vec(3));
    expect(direct.x).toBe(5);
    expect(runtime.x).toBe(5);
    expect(calls).toBe(2);
  });

  test("bound consts", () => {
    const a = new Vec(10);
    const b = new Vec(20);
    const c: any = a + b;
    expect(c.x).toBe(30);
  });

  test("different classes, primitives and reflected operands keep the runtime's answer", () => {
    const mixed: any = new Vec(1) + (5 as any);
    expect(Number.isNaN(mixed.x)).toBe(true);
    expect(new Other(1) + (new Vec(2) as any)).toBe("Other:1:false");
    expect((viaParams(7, new Other(1)) as string)).toBe("Other:1:true");
    expect(1 + 2).toBe(3);
    expect("a" + "b").toBe("ab");
  });

  test("a class without the operator coerces as before", () => {
    expect(String(new Plain() + (new Plain() as any))).toBe("[object Object][object Object]");
  });

  test("a throw in the method propagates", () => {
    let message = "";
    try {
      const p = new Boom();
      const q = new Boom();
      void (p + (q as any));
    } catch (e) {
      message = (e as Error).message;
    }
    expect(message).toBe("boom");
  });
});
