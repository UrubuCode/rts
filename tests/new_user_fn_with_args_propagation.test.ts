import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR3) Args do `new` chegam corretamente nos params da fn
// constructor. Cobre passagem de varios args com tipos diferentes.

function Point(x: number, y: number): void {
  this.x = x;
  this.y = y;
}

function Triangle(a: number, b: number, c: number): void {
  this.a = a;
  this.b = b;
  this.c = c;
  this.perimeter = a + b + c;
}

const p: any = new (Point as any)(3, 5);
const px: number = p.x;
const py: number = p.y;
print("p=(" + px + "," + py + ")");

const t: any = new (Triangle as any)(3, 4, 5);
const ta: number = t.a;
const tb: number = t.b;
const tc: number = t.c;
const tper: number = t.perimeter;
print("t=" + ta + "," + tb + "," + tc + " perim=" + tper);

describe("new UserFn com args propagados (#264 PR3)", () => {
  test("Point e Triangle recebem args corretamente", () =>
    expect(__rtsCapturedOutput).toBe(
      "p=(3,5)\n" +
      "t=3,4,5 perim=12\n"
    ));
});
