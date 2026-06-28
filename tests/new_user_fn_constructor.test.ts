import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264) `new UserFn(args)` — constructor-function ES5: `new F()` aloca a
// instância, liga `this` a ela e roda o body; `this.x = v` persiste. Via canônica
// JS `this.x`/`obj.x` (antes usava o escape hatch `collections.map_*` do motor
// velho onde a instância era um Map).

function Animal(legs: number): void {
  this.legs = legs;
}

function Plant(): void {
  this.tag = 99;
}

const a: any = new (Animal as any)(4);
const aLegs: number = a.legs;
print("aLegs=" + aLegs);

const p: any = new (Plant as any)();
const pTag: number = p.tag;
print("pTag=" + pTag);

// Cada `new` cria instancia fresh — handles distintos
const a2: any = new (Animal as any)(8);
print("distinct=" + (a !== a2));
const a2Legs: number = a2.legs;
print("a2Legs=" + a2Legs);
const aLegsAgain: number = a.legs;
print("aStill=" + aLegsAgain);

describe("new UserFn(...) constructor function (#264 PR3)", () => {
  test("aloca, this persiste, instancias distintas", () =>
    expect(__rtsCapturedOutput).toBe(
      "aLegs=4\n" +
      "pTag=99\n" +
      "distinct=true\n" +
      "a2Legs=8\n" +
      "aStill=4\n"
    ));
});
