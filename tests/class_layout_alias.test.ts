import { describe, test, expect } from "rts:test";
import { Rect as Box, Shape as Base } from "./_class_layout_shapes";

// `import { P as Q }` binds `Q` to the declaration `P` names, so what holds for
// `P` holds for `Q`. What this pins is that a renamed import is the class.

describe("a class imported under another name", () => {
  test("is that class", () => {
    let sum = 0;
    for (let i = 0; i < 1000; i++) { const b = new Box(i, 2, 3); b.grow(2); sum += b.area() + b.id; }
    expect(sum).toBe(24 * 1000 + 499500);
    const kept = [new Box(1, 2, 3), new Base(9)];
    expect(kept[0].label() + kept[1].label()).toBe("shape1shape9");
    expect(kept[0] instanceof Box && kept[0] instanceof Base).toBe(true);
    expect(Object.keys(kept[0]).join()).toBe("kind,id,w,h");
  });
});
