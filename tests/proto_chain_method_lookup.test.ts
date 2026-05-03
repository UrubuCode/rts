import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR4) Member access em instance segue __proto__ chain quando
// a key nao existe nas own props.

function Animal(name: string): void {
  collections.map_set(this as any, "id", 7);
}
Animal.prototype.legs = 4 as any;
Animal.prototype.kingdom = 1 as any;  // 1 = animal

const a: number = new (Animal as any)("Rex");

// own prop
const name: number = (a as any).id;
print("id=" + name);

// proto chain — legs e kingdom estao em Animal.prototype
const legs: number = (a as any).legs;
print("legs=" + legs);

const kingdom: number = (a as any).kingdom;
print("kingdom=" + kingdom);

// missing — nem own nem proto
const missing: number = (a as any).missing;
print("missing=" + missing);

// Outra instance: comparte mesmo prototype, suas own props sao isoladas
const b: number = new (Animal as any)("Cat");
collections.map_set(b, "id", 99);
const bName: number = (b as any).id;
const bLegs: number = (b as any).legs;
const aName: number = (a as any).id;
print("a.id=" + aName);
print("b.id=" + bName);
print("b.legs=" + bLegs);

describe("proto chain lookup em instance.field (#264 PR4)", () => {
  test("own + proto + missing + isolacao entre instances", () =>
    expect(__rtsCapturedOutput).toBe(
      "id=7\n" +
      "legs=4\n" +
      "kingdom=1\n" +
      "missing=0\n" +
      "a.id=7\n" +
      "b.id=99\n" +
      "b.legs=4\n"
    ));
});
