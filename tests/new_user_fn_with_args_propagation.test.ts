import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR3) Args do `new` chegam corretamente nos params da fn
// constructor. Cobre passagem de varios args com tipos diferentes.

function Point(x: number, y: number): void {
  collections.map_set(this as any, "x", x);
  collections.map_set(this as any, "y", y);
}

function Triangle(a: number, b: number, c: number): void {
  collections.map_set(this as any, "a", a);
  collections.map_set(this as any, "b", b);
  collections.map_set(this as any, "c", c);
  collections.map_set(this as any, "perimeter", a + b + c);
}

const p: number = new (Point as any)(3, 5);
const px: number = collections.map_get(p, "x");
const py: number = collections.map_get(p, "y");
print("p=(" + px + "," + py + ")");

const t: number = new (Triangle as any)(3, 4, 5);
const ta: number = collections.map_get(t, "a");
const tb: number = collections.map_get(t, "b");
const tc: number = collections.map_get(t, "c");
const tper: number = collections.map_get(t, "perimeter");
print("t=" + ta + "," + tb + "," + tc + " perim=" + tper);

describe("new UserFn com args propagados (#264 PR3)", () => {
  test("Point e Triangle recebem args corretamente", () =>
    expect(__rtsCapturedOutput).toBe(
      "p=(3,5)\n" +
      "t=3,4,5 perim=12\n"
    ));
});
