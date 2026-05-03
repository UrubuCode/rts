import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR3) Constructor chamado em ramos condicionais. Garante que
// THIS_PUSH/POP nao desalinha em paths que nao executam.

function Cat(): void {
  collections.map_set(this as any, "kind", 1);
}

function Dog(): void {
  collections.map_set(this as any, "kind", 2);
}

function makeAnimal(useDog: boolean): number {
  if (useDog) {
    return new (Dog as any)();
  } else {
    return new (Cat as any)();
  }
}

const a1: number = makeAnimal(true);
const a2: number = makeAnimal(false);
const k1: number = collections.map_get(a1, "kind");
const k2: number = collections.map_get(a2, "kind");
print("dog.kind=" + k1);
print("cat.kind=" + k2);

// Ternary
function Bird(): void {
  collections.map_set(this as any, "kind", 3);
}
const a3: number = (true ? new (Bird as any)() : new (Cat as any)());
const k3: number = collections.map_get(a3, "kind");
print("ternary.kind=" + k3);

describe("new UserFn em conditional/ternary (#264 PR3)", () => {
  test("if/else + ternary — this slot disciplinado", () =>
    expect(__rtsCapturedOutput).toBe(
      "dog.kind=2\n" +
      "cat.kind=1\n" +
      "ternary.kind=3\n"
    ));
});
