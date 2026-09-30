import { describe, test, expect } from "rts:test";
import { Rect, Shape, madeHere } from "./_class_layout_shapes";

// A class declared in one module and constructed in another has its layout
// there too. What this pins is that the boundary changes nothing a program can
// see: fields, methods through the chain, the prototype the instance inherits
// from, and the instance made on either side being the same kind of object.

describe("a class constructed in a module other than its own", () => {
  test("answers as the class does, whether the instance is seen or not", () => {
    let sum = 0;
    for (let i = 0; i < 1000; i++) {
      const r = new Rect(i, 2, 3);
      r.grow(2);
      sum += r.area() + r.id;
    }
    expect(sum).toBe(24 * 1000 + 499500);
    const kept = [new Rect(1, 2, 3), madeHere(4)];
    expect(kept[0].label() + kept[1].label()).toBe("shape1shape4");
    expect(kept[0].area() + kept[1].area()).toBe(6 + 16);
    expect(Object.getPrototypeOf(kept[0])).toBe(Object.getPrototypeOf(kept[1]));
    expect(kept[0] instanceof Rect && kept[0] instanceof Shape).toBe(true);
    expect(Object.keys(kept[0]).join()).toBe("kind,id,w,h");
    const s = new Shape(7);
    expect(s.label()).toBe("shape7");
  });
});
