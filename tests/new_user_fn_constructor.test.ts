import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR3) `new UserFn(args)` aloca Map, empilha como this, chama,
// retorna o Map. \`this.x = v\` no body do constructor persiste.

function Animal(legs: number): void {
  collections.map_set(this as any, "legs", legs);
}

function Plant(): void {
  collections.map_set(this as any, "tag", 99);
}

const a: number = new (Animal as any)(4);
const aLegs: number = collections.map_get(a, "legs");
print("aLegs=" + aLegs);

const p: number = new (Plant as any)();
const pTag: number = collections.map_get(p, "tag");
print("pTag=" + pTag);

// Cada `new` cria instancia fresh — handles distintos
const a2: number = new (Animal as any)(8);
print("distinct=" + (a !== a2));
const a2Legs: number = collections.map_get(a2, "legs");
print("a2Legs=" + a2Legs);
const aLegsAgain: number = collections.map_get(a, "legs");
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
