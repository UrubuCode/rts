import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR4) Member access em instance segue __proto__ chain quando
// a key nao existe nas own props. (Reescrito JS-fiel: own props via
// `this.id = ...` — o escape-hatch collections.map_set(this) era o
// modelo velho de instancia-como-Map; missing agora le undefined.)

function Animal(id: number): void {
  (this as any).id = id;
}
Animal.prototype.legs = 4 as any;
Animal.prototype.kingdom = 1 as any;  // 1 = animal

const a: any = new (Animal as any)(7);

// own prop
print("id=" + a.id);

// proto chain — legs e kingdom estao em Animal.prototype
print("legs=" + a.legs);
print("kingdom=" + a.kingdom);

// missing — nem own nem proto (JS: undefined)
print("missing=" + a.missing);

// Outra instance: comparte mesmo prototype, suas own props sao isoladas
const b: any = new (Animal as any)(99);
print("a.id=" + a.id);
print("b.id=" + b.id);
print("b.legs=" + b.legs);

describe("proto chain lookup em instance.field (#264 PR4)", () => {
  test("own + proto + missing + isolacao entre instances", () =>
    expect(__rtsCapturedOutput).toBe(
      "id=7\n" +
      "legs=4\n" +
      "kingdom=1\n" +
      "missing=undefined\n" +
      "a.id=7\n" +
      "b.id=99\n" +
      "b.legs=4\n"
    ));
});
