import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR2) `Animal.prototype.x = value` persiste no objeto prototype da
// classe/fn. (Reescrito JS-fiel: leitura direta `Animal.prototype.x` — o
// collections.map_get(proto) era o modelo velho de proto-como-Map.)

function Animal(): void {}
function Plant(): void {}

// 1. Assign direto + leitura direta
Animal.prototype.color = 7 as any;
print("color=" + (Animal.prototype as any).color);

// 2. Assign via cast `as any`
(Animal as any).prototype.weight = 42;
print("weight=" + (Animal.prototype as any).weight);

// 3. Multiplos campos no mesmo proto
Animal.prototype.legs = 4 as any;
Animal.prototype.tail = 1 as any;
print("legs=" + (Animal.prototype as any).legs);
print("tail=" + (Animal.prototype as any).tail);

// 4. Protos distintos por fn
Plant.prototype.leaves = 99 as any;
print("plantLeaves=" + (Plant.prototype as any).leaves);
print("animalLeaves=" + (Animal.prototype as any).leaves);

// 5. Reescrita do mesmo campo
Animal.prototype.color = 99 as any;
print("colorAfter=" + (Animal.prototype as any).color);

describe("fn.prototype assignment (#264 PR2)", () => {
  test("assign + read + multiplos campos + protos distintos + reescrita", () =>
    expect(__rtsCapturedOutput).toBe(
      "color=7\n" +
      "weight=42\n" +
      "legs=4\n" +
      "tail=1\n" +
      "plantLeaves=99\n" +
      "animalLeaves=undefined\n" +
      "colorAfter=99\n"
    ));
});
