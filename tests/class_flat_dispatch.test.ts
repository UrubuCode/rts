import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #147 passo 8: dispatch virtual em classes flat (layout nativo).
// Nomes com prefixo `__Flat` sao opt-in para o caminho flat
// (is_class_flat_enabled). O override de `sound()` em `__FlatDog`
// precisa ser visivel mesmo quando o receiver e tipado como `__FlatAnimal`
// — virtual dispatch real le o tag via `gc.instance_class`.
class __FlatAnimal {
  name: string;
  constructor(n: string) {
    this.name = n;
  }
  sound(): string {
    return "generic";
  }
}

class __FlatDog extends __FlatAnimal {
  constructor(n: string) {
    super(n);
  }
  sound(): string {
    return "woof";
  }
}

const a: __FlatAnimal = new __FlatAnimal("X");
const d: __FlatDog = new __FlatDog("Rex");
const polymorphic: __FlatAnimal = d;
print(a.sound());
print(d.sound());
print(polymorphic.sound());

describe("fixture:class_flat_dispatch", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("generic\nwoof\nwoof\n");
  });
});
