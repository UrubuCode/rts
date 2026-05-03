import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Arrow com destructuring de object no param.

type Pt = { x: number; y: number };
const points: Pt[] = [{ x: 1, y: 10 }, { x: 2, y: 20 }];

const sumPt = ({ x, y }: Pt): number => x + y;
const r1: number = sumPt(points[0]);
const r2: number = sumPt(points[1]);
print("a=" + r1);
print("b=" + r2);

describe("arrow com destructuring de object param", () => {
  test("{x,y} destructure prologue funciona", () =>
    expect(__rtsCapturedOutput).toBe(
      "a=11\n" +
      "b=22\n"
    ));
});
