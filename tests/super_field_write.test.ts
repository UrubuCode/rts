import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// `super.x = v` escreve no RECEPTOR, nao no prototipo do pai — OrdinarySet
// procura um setter a partir do prototipo e, nao o achando, cria a propriedade
// em `this`. Por isso a primeira linha e 42.
//
// A segunda linha era 142 e esta errada: `super.x` LE do prototipo, onde nada
// foi escrito (a escrita anterior foi para a instancia), logo `undefined + 100`
// e NaN. Confirmado no Node. O ficheiro afirmava um `super` que le de volta o
// que escreveu, que nenhum motor faz.

class Base {
    x: number = 0;
}

class Sub extends Base {
    setBase(v: number): void {
        super.x = v;
    }
    setBaseCompound(): void {
        super.x = super.x + 100;
    }
}

const s = new Sub();
s.setBase(42);
print(`${s.x}`); // 42

s.setBaseCompound(); // super.x le undefined do prototipo: undefined + 100
print(`${s.x}`); // NaN

describe("fixture:super_field_write", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\nNaN\n");
  });
});
