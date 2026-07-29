import { describe, test, expect } from "rts:test";

// A class's identity is NOMINAL, not structural. An object that merely happens to
// carry the same ordered field list must NOT satisfy `instanceof`, and must NOT
// be able to call the class's methods.
//
// Regression for the soundness defect where `intern_class_shape` published the
// class's shape id into the content-addressed `by_keys` layout map, so any object
// interning the same ordered key list was handed the class's identity — and
// `class/vdispatch.rs`, which dispatches on a flat compare of the slot-0 shape
// word with no constructor check, then executed the class's methods against it.
// Reachable from `JSON.parse`, i.e. from external data.

class Point {
  x: number;
  y: number;
  constructor(a: number, b: number) {
    this.x = a;
    this.y = b;
  }
  greet(): string {
    return "Point method x=" + this.x;
  }
}

// A second class with the SAME ordered field list: class identity is per
// DECLARATION, so these two must never be confused with each other either.
class Vec2 {
  x: number;
  y: number;
  constructor(a: number, b: number) {
    this.x = a;
    this.y = b;
  }
}

const real = new Point(1, 2);
const vec = new Vec2(1, 2);

// Every route that reaches the layout interner with Point's ordered key list.
const dynamicAdds: any = {};
dynamicAdds.x = 42;
dynamicAdds.y = 7;

const literal: any = { x: 1, y: 2 };
const spread: any = { ...real };
const assigned: any = Object.assign({}, real);
const parsed: any = JSON.parse('{"x":1,"y":2}');

// The class's method body must never RUN against these objects. It returns
// "Point method x=<n>" when it does, so any other result means it did not.
//
// NOTE: bun throws `TypeError: o.greet is not a function` here; RTS currently
// yields `undefined` instead, because the dynamic method-dispatch DEFAULT arm
// (`class/vdispatch.rs`, `try_user_virtual_dynamic`) keeps an `undefined`
// sentinel when the receiver has no own function-valued property of that name,
// rather than throwing. That is a SEPARATE pre-existing gap, tracked apart from
// this regression: what this test pins down is that the class method does not
// execute, which is the soundness property.
function callsGreet(o: any): string {
  try {
    return String(o.greet());
  } catch (e) {
    return "threw";
  }
}

function ranClassMethod(result: string): boolean {
  return result.indexOf("Point method") === 0;
}

const dynGreet = callsGreet(dynamicAdds);
const litGreet = callsGreet(literal);
const spreadGreet = callsGreet(spread);
const assignGreet = callsGreet(assigned);
const parsedGreet = callsGreet(parsed);

describe("class identity is nominal, not structural", () => {
  test("a real instance keeps its identity and methods", () => {
    expect(real instanceof Point).toBe(true);
    expect(real.greet()).toBe("Point method x=1");
  });

  test("dynamically-grown object is not an instance", () => {
    expect(dynamicAdds instanceof Point).toBe(false);
  });

  test("object literal with the same fields is not an instance", () => {
    expect(literal instanceof Point).toBe(false);
  });

  test("spread of an instance is a plain object", () => {
    expect(spread instanceof Point).toBe(false);
  });

  test("Object.assign copy of an instance is a plain object", () => {
    expect(assigned instanceof Point).toBe(false);
  });

  test("JSON.parse output is not an instance", () => {
    expect(parsed instanceof Point).toBe(false);
  });

  test("none of them executes the class method body", () => {
    expect(ranClassMethod(dynGreet)).toBe(false);
    expect(ranClassMethod(litGreet)).toBe(false);
    expect(ranClassMethod(spreadGreet)).toBe(false);
    expect(ranClassMethod(assignGreet)).toBe(false);
    expect(ranClassMethod(parsedGreet)).toBe(false);
  });

  test("reverse key order is also not an instance", () => {
    const reversed: any = {};
    reversed.y = 7;
    reversed.x = 42;
    expect(reversed instanceof Point).toBe(false);
  });

  test("two classes with identical fields stay distinct", () => {
    expect(vec instanceof Vec2).toBe(true);
    expect(vec instanceof Point).toBe(false);
    expect(real instanceof Vec2).toBe(false);
  });
});
