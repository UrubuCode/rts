import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR3) Constructor chamado em ramos condicionais. Garante que
// THIS_PUSH/POP nao desalinha em paths que nao executam.
//
// `collections.map_set(this, "kind", n)` / `map_get(h, "kind")` do namespace
// `rts` eram, aqui, escrita e leitura de UMA propriedade no objeto recem
// construido — nao um Map por chave arbitraria. A superficie que fica exprime
// isso diretamente com property access, que e' o que a assercao sempre pinou:
// o `this` do constructor e' o objeto que `new` devolve, mesmo em ramo
// condicional.

function Cat(): void {
  (this as any).kind = 1;
}

function Dog(): void {
  (this as any).kind = 2;
}

function makeAnimal(useDog: boolean): any {
  if (useDog) {
    return new (Dog as any)();
  } else {
    return new (Cat as any)();
  }
}

const a1: any = makeAnimal(true);
const a2: any = makeAnimal(false);
const k1: number = (a1 as any).kind;
const k2: number = (a2 as any).kind;
print("dog.kind=" + k1);
print("cat.kind=" + k2);

// Ternary
function Bird(): void {
  (this as any).kind = 3;
}
const a3: any = (true ? new (Bird as any)() : new (Cat as any)());
const k3: number = (a3 as any).kind;
print("ternary.kind=" + k3);

describe("new UserFn em conditional/ternary (#264 PR3)", () => {
  test("if/else + ternary — this slot disciplinado", () =>
    expect(__rtsCapturedOutput).toBe(
      "dog.kind=2\n" +
      "cat.kind=1\n" +
      "ternary.kind=3\n"
    ));
});
