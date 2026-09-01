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

describe("an `instanceof` does not spend the receiver", () => {
  // It walks a prototype chain and reads `C.prototype`. It writes nothing and
  // hands neither operand anywhere, so the clause the value-read rule exists for
  // — that a read is a way of reaching something which could reassign `o.m` —
  // does not apply.
  //
  // It matters because it is written: `bench/analytic.ts` reads
  // `derived instanceof Base` in one row and calls `derived.bp()` in another,
  // and the second stayed a real call at 22.00 ns for the first one's sake.
  // With the exemption it is 3.00.
  class Walked {
    w(): number {
      return 3;
    }
  }
  const walked = new Walked();

  test("the call is decided even though the receiver is tested", () => {
    let seen = 0;
    for (let i = 0; i < 3; i++) {
      if (walked instanceof Walked) seen += walked.w();
    }
    expect(seen).toBe(9);
  });

  test("and the test itself still answers", () => {
    expect(walked instanceof Walked).toBe(true);
    expect(walked instanceof Error).toBe(false);
  });

  test("a class defining `Symbol.hasInstance` is NOT exempt", () => {
    // `instanceof` DEFERS to it, called with the left operand — user code
    // holding the receiver, which may write through it. The handler below does
    // exactly that, and the call after it must see the write.
    class Hooked {
      static [Symbol.hasInstance](x: any): boolean {
        x.m = () => 99;
        return true;
      }
      m(): number {
        return 1;
      }
    }
    const hooked: any = new Hooked();
    expect(hooked.m()).toBe(1);
    expect(hooked instanceof Hooked).toBe(true);
    expect(hooked.m()).toBe(99);
  });
});

describe("nine wrong answers, and the premise they broke", () => {
  // `receiver.rs` shipped with a header claiming that every way of changing what
  // `o.m` reaches has to SPELL `o` or `C`, so a walk over the program's use of
  // those two names finds them all. IT IS FALSE. An adversarial survey produced
  // nine programs where this engine disagreed with node, every one of them
  // ordinary code, and four came from one cause: `this` inside any body of the
  // chain is a THIRD spelling of the receiver, which no walk over `o` can see.
  //
  // Each test below is one of those programs. They are grouped rather than
  // spread because they are one defect with one repair, and because a reader who
  // changes the chain walk needs them all in one place.

  test("a FIELD shadows a base method — an own property beats the chain", () => {
    class B {
      m(): number {
        return 1;
      }
    }
    class D extends B {
      m = (): number => 2;
    }
    expect(new D().m()).toBe(2);
  });

  test("and it beats a DERIVED method too, at any level", () => {
    // The field is installed on the instance by the base's constructor, so it
    // shadows the derived prototype's method. A rule that stopped at the first
    // class to mention the key would answer 2 here.
    class B {
      m = (): number => 1;
    }
    class D extends B {
      m(): number {
        return 2;
      }
    }
    expect(new D().m()).toBe(1);
  });

  test("a derived GETTER shadows a base method", () => {
    class B {
      m(): number {
        return 1;
      }
    }
    class D extends B {
      get m(): any {
        return () => 2;
      }
    }
    expect(new D().m()).toBe(2);
  });

  test("a COMPUTED key can name anything, so the class settles nothing", () => {
    class C {
      m(): number {
        return 1;
      }
      ["m"](): number {
        return 2;
      }
    }
    expect(new C().m()).toBe(2);
  });

  test("a constructor that RETURNS makes the instance somebody else's", () => {
    const other = {
      m(): number {
        return 99;
      },
    };
    class C {
      constructor() {
        return other as any;
      }
      m(): number {
        return 1;
      }
    }
    expect(new C().m()).toBe(99);
  });

  test("`this.m = f` from a sibling method", () => {
    class C {
      m(): number {
        return 1;
      }
      patch(): void {
        (this as any).m = () => 2;
      }
    }
    const o = new C();
    o.patch();
    expect(o.m()).toBe(2);
  });

  test("`this.m = f` from the constructor", () => {
    class C {
      m(): number {
        return 1;
      }
      constructor() {
        (this as any).m = () => 2;
      }
    }
    expect(new C().m()).toBe(2);
  });

  test("`this` RETURNED, and written through outside", () => {
    class C {
      m(): number {
        return 1;
      }
      self(): any {
        return this;
      }
    }
    const o = new C();
    o.self().m = () => 2;
    expect(o.m()).toBe(2);
  });

  test("`this` handed to a function as an ARGUMENT", () => {
    function grab(x: any): void {
      x.m = () => 2;
    }
    class C {
      m(): number {
        return 1;
      }
      give(): void {
        grab(this);
      }
    }
    const o = new C();
    o.give();
    expect(o.m()).toBe(2);
  });

  test("`o.constructor()` is still a TypeError", () => {
    class C {
      m(): number {
        return 1;
      }
    }
    const o = new C();
    let threw = "";
    try {
      (o as any).constructor();
    } catch (err) {
      threw = (err as Error).constructor.name;
    }
    expect(threw).toBe("TypeError");
  });
});

describe("a PARAMETER binds the spelling too", () => {
  // A tenth wrong answer, found by the same survey and living past the fix for
  // the other nine. `receiver.rs` counted DECLARATIONS with a counter of its own
  // — a class, a `const`, a `function` — and a parameter is none of those. The
  // map is keyed by spelling, so a decided `const o = new C()` anywhere in the
  // module answered C's method for `function g(o) { return o.m(); }`.
  //
  //   g({ m: () => "the argument's" })   rts "class C method"   node "the argument's"
  //
  // The repair is reuse rather than a wider counter: `inline::declarations_of`
  // already counts a parameter, a `catch` binding and a loop target, and says in
  // its own comment that over-counting is the safe direction. Two counters were
  // two chances to disagree about what a binding is.
  class Held {
    m(): string {
      return "the class method";
    }
  }
  const held = new Held();

  test("the parameter's own object answers, and the decided receiver still answers", () => {
    function viaParameter(held: any): string {
      return held.m();
    }
    expect(viaParameter({ m: () => "the argument's" })).toBe("the argument's");
    expect(held.m()).toBe("the class method");
  });

  test("a `catch` binding of the same spelling", () => {
    function thrown(): string {
      try {
        throw { m: () => "the caught one's" };
      } catch (held: any) {
        return held.m();
      }
    }
    expect(thrown()).toBe("the caught one's");
  });

  test("a loop target of the same spelling", () => {
    function looped(): string {
      let seen = "";
      for (const held of [{ m: () => "the element's" }]) seen = held.m();
      return seen;
    }
    expect(looped()).toBe("the element's");
  });
});
