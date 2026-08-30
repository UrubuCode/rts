// A method call is one runtime crossing. Measured 2026-08-30, release, min of 9:
//
//   callee.m(a)                            19.00 ns
//   the same function through a binding     18.00 ns
//   the property read alone                  6.00 ns
//   a substituted call                       1.00 ns
//
// Being a method costs about one nanosecond, so what a substitution removes is
// the crossing. `receiver.rs` decides which `o.m` names one function without a
// guard and without a cache: `const o = new C()` where neither `o` nor `C` is
// ever read as a value, so nothing can reassign `o.m` or `C.prototype` without
// spelling one of them.
//
//   callee.m(a)              22.33 -> 4.33
//   derived.bp()             22.00 -> 3.00
//
// Every test here is about a VALUE, and the refusals outnumber the wins,
// because a wrong resolution answers the wrong function rather than crashing.
import { describe, test, expect } from "rts:test";

class Counter {
  v: number = 1;
  m(x: number): number {
    return x + this.v;
  }
  zero(): number {
    return 0;
  }
}
const counter = new Counter();

class Base {
  bp(): number {
    return 1;
  }
  shared(): number {
    return 10;
  }
}
class Derived extends Base {
  x: number = 2;
  shared(): number {
    return 20;
  }
}
const derived = new Derived();

describe("a method the program decides", () => {
  test("`this` is the receiver, and its field is read", () => {
    expect(counter.m(5)).toBe(6);
    expect(counter.m(0)).toBe(1);
  });

  test("a method that ignores `this`", () => {
    expect(counter.zero()).toBe(0);
  });

  test("one inherited from the base", () => {
    expect(derived.bp()).toBe(1);
  });

  test("a derived method SHADOWS the base's", () => {
    // The chain is walked from the derived class, and the first answer wins.
    // If the base's were taken this is 10.
    expect(derived.shared()).toBe(20);
  });

  test("called in a loop, which is the measured shape", () => {
    let a = 0;
    for (let i = 0; i < 4; i++) a = counter.m(a);
    expect(a).toBe(4);
  });

  test("`this` inside the body is the receiver and not the caller's", () => {
    const outer = { v: 999 };
    function inside(this: any): number {
      return counter.m(0);
    }
    expect(inside.call(outer)).toBe(1);
  });
});

describe("what the resolution must refuse", () => {
  test("a receiver read as a value", () => {
    // Anything reached through that read could write `g`.
    class Loose {
      g(): number {
        return 7;
      }
    }
    const loose = new Loose();
    function leak(o: any): any {
      o.g = () => 99;
      return o;
    }
    expect(leak(loose).g()).toBe(99);
  });

  test("a class written through", () => {
    class Written {
      w(): number {
        return 1;
      }
    }
    const written = new Written();
    (Written.prototype as any).w = () => 42;
    expect(written.w()).toBe(42);
  });

  test("a `let` receiver, which may be reassigned", () => {
    class A {
      k(): number {
        return 1;
      }
    }
    class B {
      k(): number {
        return 2;
      }
    }
    let held: any = new A();
    expect(held.k()).toBe(1);
    held = new B();
    expect(held.k()).toBe(2);
  });

  test("a static method is not on the instance", () => {
    class WithStatic {
      static s(): number {
        return 1;
      }
      i(): number {
        return 2;
      }
    }
    const held = new WithStatic();
    expect((held as any).s).toBe(undefined);
    expect(held.i()).toBe(2);
  });

  test("a getter is not a method call", () => {
    class Gets {
      get g(): number {
        return 5;
      }
    }
    const gets = new Gets();
    expect(gets.g).toBe(5);
  });

  test("an own property shadows the prototype's method", () => {
    class Shadowed {
      s(): number {
        return 1;
      }
    }
    const held: any = new Shadowed();
    held.s = () => 2;
    expect(held.s()).toBe(2);
  });

  test("a method reading `arguments` reads its own", () => {
    class Args {
      count(): number {
        // eslint-disable-next-line prefer-rest-params
        return arguments.length;
      }
    }
    const args = new Args();
    expect((args as any).count(1, 2, 3)).toBe(3);
  });
});
