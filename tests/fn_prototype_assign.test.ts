import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR2) `Animal.prototype.x = value` agora persiste no Map prototype.

function Animal(): void {}
function Plant(): void {}

// 1. Assign direto
Animal.prototype.color = 7 as any;
const proto: number = Animal.prototype as any;
const color: number = collections.map_get(proto, "color");
print("color=" + color);

// 2. Assign via cast `as any`
(Animal as any).prototype.weight = 42;
const weight: number = collections.map_get(proto, "weight");
print("weight=" + weight);

// 3. Multiplos campos no mesmo proto
Animal.prototype.legs = 4 as any;
Animal.prototype.tail = 1 as any;
const legs: number = collections.map_get(proto, "legs");
const tail: number = collections.map_get(proto, "tail");
print("legs=" + legs);
print("tail=" + tail);

// 4. Protos distintos por fn (cada Map separado)
Plant.prototype.leaves = 99 as any;
const plantProto: number = Plant.prototype as any;
const leaves: number = collections.map_get(plantProto, "leaves");
const animalLeaves: number = collections.map_get(proto, "leaves");
print("plantLeaves=" + leaves);
print("animalLeaves=" + animalLeaves);

// 5. Reescrita do mesmo campo
Animal.prototype.color = 99 as any;
const colorAfter: number = collections.map_get(proto, "color");
print("colorAfter=" + colorAfter);

describe("fn.prototype assignment (#264 PR2)", () => {
  test("assign + read + multiplos campos + protos distintos + reescrita", () =>
    expect(__rtsCapturedOutput).toBe(
      "color=7\n" +
      "weight=42\n" +
      "legs=4\n" +
      "tail=1\n" +
      "plantLeaves=99\n" +
      "animalLeaves=0\n" +
      "colorAfter=99\n"
    ));
});
