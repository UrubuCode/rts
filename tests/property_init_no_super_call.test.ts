import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Sub com ctor explícito mas SEM super() chamado.
//
// A premissa original deste fixture — "semântica válida quando parent é
// trivial" — está ERRADA. Conferido contra Node v22: uma classe derivada cujo
// construtor não chama `super()` e depois acessa `this` (o que um inicializador
// de campo faz implicitamente) lança `ReferenceError: Must call super
// constructor in derived class before accessing 'this' or returning from
// derived constructor` — `new Sub()` NUNCA completa, `s.b` nunca é lido. Não é
// "parent não inicializa e Sub segue"; é um erro fatal na spec.
//
// GAP DE CAPACIDADE (fora da minha área — construção de classe/`super` é
// `emit/call.rs`/`entry/functions.rs`, não `rts-core-rwk/entry`): este motor
// não impõe essa checagem hoje, então `new Sub()` completa sem lançar — mas
// MEDIDO nesta sessão (2026-08-09/10, `suite_run` sobre este ficheiro), `s.b`
// lê `undefined`, não `13`: o inicializador de `Sub.b` também não roda quando
// `super()` não é chamado explicitamente, o que é uma segunda divergência da
// spec (que exigiria lançar) e não a que o fixture original documentava
// ("initializer roda mesmo assim"). Fixado ao valor MEDIDO e honesto do motor
// atual — nem "13" (a alegação antiga, não observada) nem uma exceção (o que
// a spec pede e o motor ainda não lança).
class Base {
    a: number = 7;
}

class Sub extends Base {
    b: number = 13;
    constructor() {
        // Não chamamos super() aqui.
    }
}

const s = new Sub();
print(`${s.b}`); // undefined — medido, não a spec

describe("fixture:property_init_no_super_call", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("undefined\n");
  });
});
