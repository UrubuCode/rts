import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// super.field NAO alcanca um campo de instancia, e este ficheiro afirmava que
// sim. `super.x` faz OrdinaryGet a partir do PROTOTIPO do pai; um campo escrito
// por um inicializador vive na INSTANCIA e nunca no `prototype`, portanto a
// leitura responde `undefined` e a soma responde NaN. Confirmado no Node:
//   class Base { x = 7 } class Sub extends Base { s(){ return super.x } }
//   new Sub().s()   // undefined
// A expectativa antiga (20) so seria produzivel por um motor que redirigisse
// `super` para o receptor — o oposto do que `super` existe para fazer.

class Base {
    x: number = 7;
    y: number = 13;
}

class Sub extends Base {
    sumViaSuper(): number {
        return super.x + super.y; // NaN — nenhum dos dois esta no prototipo
    }
    sumViaThis(): number {
        return this.x + this.y; // 20 — equivalente
    }
}

const s = new Sub();
print(`${s.sumViaSuper()}`);
print(`${s.sumViaThis()}`);

describe("fixture:super_field_read", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("NaN\n20\n");
  });
});
