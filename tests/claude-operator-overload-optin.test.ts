import { describe, test, expect } from "rts:test";
import { operators } from "rts";

// Operator overloading is opt-in: only an object that declares one of the
// symbols `rts` exports answers an operator. This file imports `rts`, so the
// runtime's check is ARMED here — every "unchanged" case below is measured
// with the check live, which is the case that could regress.

let calls = 0;

class Money {
  cents: number;
  constructor(cents: number) {
    this.cents = cents;
  }
  [operators.add](other: any, reversed: boolean): Money {
    calls++;
    const cents = other instanceof Money ? other.cents : other * 100;
    return new Money(this.cents + cents);
  }
  [operators.sub](other: any, reversed: boolean): Money {
    calls++;
    const cents = other instanceof Money ? other.cents : other * 100;
    return new Money(reversed ? cents - this.cents : this.cents - cents);
  }
  [operators.mul](k: number, reversed: boolean): Money {
    calls++;
    return new Money(this.cents * k);
  }
  [operators.div](k: number, reversed: boolean): Money {
    calls++;
    return new Money(this.cents / k);
  }
  [operators.mod](k: number, reversed: boolean): Money {
    calls++;
    return new Money(this.cents % k);
  }
  [operators.pow](k: number, reversed: boolean): string {
    calls++;
    return reversed ? `${k}**money` : `money**${k}`;
  }
  [operators.lt](other: Money, reversed: boolean): boolean {
    calls++;
    return reversed ? other.cents < this.cents : this.cents < other.cents;
  }
  [operators.le](other: Money, reversed: boolean): number {
    calls++;
    // Deliberately not a boolean: the operator answers ToBoolean of it.
    return (reversed ? other.cents <= this.cents : this.cents <= other.cents) ? 1 : 0;
  }
  [operators.eq](other: Money, reversed: boolean): boolean {
    calls++;
    return this.cents === other.cents;
  }
  [operators.neg](): Money {
    calls++;
    return new Money(-this.cents);
  }
  valueOf(): number {
    return this.cents;
  }
}

// Declares `gt` and `ge` only, to show each symbol answers its own operator.
class OnlyGreater {
  n: number;
  constructor(n: number) {
    this.n = n;
  }
  [operators.gt](other: OnlyGreater, reversed: boolean): boolean {
    calls++;
    return reversed ? other.n > this.n : this.n > other.n;
  }
  [operators.ge](other: OnlyGreater, reversed: boolean): boolean {
    calls++;
    return reversed ? other.n >= this.n : this.n >= other.n;
  }
  valueOf(): number {
    return 1000 + this.n;
  }
}

class Thrower {
  [operators.add](other: any, reversed: boolean): never {
    throw new Error("add refused");
  }
}

class Base {
  v: number;
  constructor(v: number) {
    this.v = v;
  }
  [operators.mul](k: number, reversed: boolean): number {
    calls++;
    return this.v * k * 10;
  }
}
class Derived extends Base {}

// Methods NAMED like operators, and no symbol: the old engine's form. Must
// not be called by any operator.
class Named {
  add(other: any): string {
    calls++;
    return "WRONG add";
  }
  sub(other: any): string {
    calls++;
    return "WRONG sub";
  }
  toString(): string {
    return "named";
  }
}

describe("operator overloading is opt-in", () => {
  test("Set, Map and a class with add/sub methods are unchanged", () => {
    calls = 0;
    const set = new Set([1, 2]);
    const map = new Map([["a", 1]]);
    expect((set as any) + "").toBe("[object Set]");
    expect((map as any) + "").toBe("[object Map]");
    expect(Number.isNaN((set as any) - 1)).toBe(true);
    const n = new Named();
    expect((n as any) + "!").toBe("named!");
    expect(Number.isNaN((n as any) - (n as any))).toBe(true);
    expect(calls).toBe(0);
  });

  test("a primitive with a primitive is the specification's answer", () => {
    expect(1 + 2).toBe(3);
    expect("a" + 1).toBe("a1");
    expect(7 % 4).toBe(3);
    expect(2 ** 10).toBe(1024);
    expect("b" < "a").toBe(false);
    expect(-(3 as any)).toBe(-3);
    expect((1 as any) == "1").toBe(true);
  });

  test("an object with valueOf and no symbol still follows ToPrimitive", () => {
    const o = { valueOf: () => 41 };
    expect((o as any) + 1).toBe(42);
    expect((o as any) * 2).toBe(82);
    expect((o as any) < 50).toBe(true);
    expect(-(o as any)).toBe(-41);
    expect((o as any) == 41).toBe(true);
  });

  test("every arithmetic symbol answers its own operator, and reflects", () => {
    calls = 0;
    const m = new Money(500);
    expect(((m as any) + new Money(25)).cents).toBe(525);
    expect(((m as any) + 1).cents).toBe(600);
    expect((1 + (m as any)).cents).toBe(600);
    expect(((m as any) - 1).cents).toBe(400);
    expect((10 - (m as any)).cents).toBe(500);
    expect(((m as any) * 3).cents).toBe(1500);
    expect((3 * (m as any)).cents).toBe(1500);
    expect(((m as any) / 5).cents).toBe(100);
    expect(((m as any) % 7).cents).toBe(3);
    expect((m as any) ** 2).toBe("money**2");
    expect(2 ** (m as any)).toBe("2**money");
    expect(calls).toBe(11);
  });

  test("a method that throws is caught like any throw", () => {
    let caught = "";
    try {
      const r = (new Thrower() as any) + 1;
      caught = "not thrown: " + r;
    } catch (e: any) {
      caught = e.message;
    }
    expect(caught).toBe("add refused");
    // And the reflected side throws the same way.
    try {
      caught = "";
      const r = 1 + (new Thrower() as any);
      caught = "not thrown: " + r;
    } catch (e: any) {
      caught = e.message;
    }
    expect(caught).toBe("add refused");
  });

  test("relational operators: each symbol its own, no derivation, ToBoolean", () => {
    calls = 0;
    const small = new Money(1);
    const big = new Money(2);
    expect((small as any) < (big as any)).toBe(true);
    expect((big as any) < (small as any)).toBe(false);
    expect((small as any) <= (big as any)).toBe(true);
    expect(calls).toBe(3);
    // Money declares no `gt`: `>` is NOT `!(<=)`, it is ToPrimitive (valueOf).
    expect((big as any) > (small as any)).toBe(true);
    expect(calls).toBe(3);

    const g1 = new OnlyGreater(1);
    const g2 = new OnlyGreater(2);
    expect((g2 as any) > (g1 as any)).toBe(true);
    expect((g1 as any) >= (g2 as any)).toBe(false);
    expect(calls).toBe(5);
    // `<` is not derived from `gt`: valueOf answers 1001 < 1002.
    expect((g1 as any) < (g2 as any)).toBe(true);
    expect(calls).toBe(5);
    // Reflected: a number on the left asks the right operand, reversed.
    expect((5 as any) > (g1 as any)).toBe(false);
    expect(calls).toBe(6);
  });

  test("== against null never calls the method; eq needs two objects", () => {
    calls = 0;
    const m = new Money(5);
    expect((m as any) == null).toBe(false);
    expect((m as any) != undefined).toBe(true);
    expect(null == (m as any)).toBe(false);
    // One side a primitive: the specification's ToPrimitive, not `eq`.
    expect((m as any) == 5).toBe(true);
    expect(calls).toBe(0);
    expect((m as any) == (new Money(5) as any)).toBe(true);
    expect((m as any) != (new Money(6) as any)).toBe(true);
    expect(calls).toBe(2);
  });

  test("+= and -= go through the same operator", () => {
    calls = 0;
    let total: any = new Money(100);
    total += 2;
    total -= new Money(50);
    expect(total.cents).toBe(250);
    expect(calls).toBe(2);
  });

  test("unary minus answers neg; unary plus and ++ are NOT mul", () => {
    calls = 0;
    const m = new Money(7);
    expect((-(m as any)).cents).toBe(-7);
    expect(calls).toBe(1);
    // `+m` is ToNumber (valueOf), never `m * 1`.
    expect(+(m as any)).toBe(7);
    let counter: any = new Money(9);
    counter++;
    expect(counter).toBe(10);
    expect(calls).toBe(1);
  });

  test("a symbol inherited through extends is found", () => {
    calls = 0;
    expect((new Derived(2) as any) * 3).toBe(60);
    expect(calls).toBe(1);
  });

  test("=== and !== are identity, never overloaded", () => {
    calls = 0;
    const a = new Money(1);
    const b = new Money(1);
    expect(a === b).toBe(false);
    expect(a !== b).toBe(true);
    expect(a === a).toBe(true);
    expect(calls).toBe(0);
  });

  test("the symbols are unique, described, and not in the global registry", () => {
    expect(typeof operators.add).toBe("symbol");
    expect(operators.add.description).toBe("rts.operators.add");
    expect(operators.add === operators.sub).toBe(false);
    expect(Symbol.for("rts.operators.add") === operators.add).toBe(false);
    expect(Symbol.keyFor(operators.add)).toBe(undefined);
  });
});
