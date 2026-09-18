import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Sub com ctor explícito mas SEM super() chamado.
//
// Uma classe derivada cujo construtor termina sem chamar `super()` lança
// `ReferenceError` — o fim do corpo é um `return this` implícito, e o `this`
// de um construtor derivado só existe depois de `super()`. `new Sub()` NUNCA
// completa e `s.b` nunca é lido. Conferido com este mesmo programa no Node 20
// ("Must call super constructor in derived class before accessing 'this' or
// returning from derived constructor") e no Bun 1.4.
//
// Este fixture já afirmou duas coisas erradas: primeiro "o inicializador corre
// mesmo assim" (13), depois o valor medido do motor (`undefined`), que deixou
// de o ser quando `new Sub()` passou a responder `undefined` e o programa
// morreu uma linha depois a ler `.b` de nada. O que a especificação pede é a
// exceção, e é o que se afirma agora.
class Base {
    a: number = 7;
}

class Sub extends Base {
    b: number = 13;
    constructor() {
        // Não chamamos super() aqui.
    }
}

try {
    const s = new Sub();
    print(`constructed ${s.b}`);
} catch (e) {
    print((e as Error).constructor.name);
}

describe("fixture:property_init_no_super_call", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("ReferenceError\n");
  });
});
