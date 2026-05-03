import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264) Substituir fn.prototype atualiza o registry e instances
// novas usam o novo Map para chain.

function Animal(): void {}
Animal.prototype.legs = 4 as any;

function Dog(): void {}
// Dog.prototype = Object.create(Animal.prototype) — chain Dog -> Animal
Dog.prototype = Object.create(Animal.prototype as any) as any;
Dog.prototype.barks = 1 as any;

const d: number = new (Dog as any)();
const legs: number = (d as any).legs;
const barks: number = (d as any).barks;
const missing: number = (d as any).xyz;
print("legs=" + legs);
print("barks=" + barks);
print("missing=" + missing);

describe("fn.prototype set explicito (#264)", () => {
  test("prototype = Object.create(parent) cria chain 2 niveis", () =>
    expect(__rtsCapturedOutput).toBe(
      "legs=4\n" +
      "barks=1\n" +
      "missing=0\n"
    ));
});
