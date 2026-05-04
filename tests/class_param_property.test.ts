import { describe, test, expect } from "rts:test";

// (#585) TypeScript parameter properties: public/private/protected/readonly
// no constructor declaram + atribuem field automaticamente.

class Point {
  constructor(public x: number, public y: number) {}
  distance(): number {
    return Math.sqrt(this.x * this.x + this.y * this.y);
  }
}

class Circle {
  constructor(public radius: number) {}
  area(): number {
    return 3.14159 * this.radius * this.radius;
  }
}

class WithReadonly {
  constructor(readonly id: number) {}
}

const p = new Point(3, 4);
const px = p.x;
const py = p.y;
const pd = p.distance();

const c = new Circle(5);
const cr = c.radius;
const ca = c.area();

const w = new WithReadonly(99);
const wid = w.id;

describe("class_param_property", () => {
  test("public x", () => expect(px).toBe(3));
  test("public y", () => expect(py).toBe(4));
  test("usa this.x", () => expect(pd).toBe(5));
  test("Circle.radius", () => expect(cr).toBe(5));
  test("readonly id", () => expect(wid).toBe(99));
});
