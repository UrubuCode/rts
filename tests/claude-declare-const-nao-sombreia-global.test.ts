import { describe, test, expect } from "rts:test";

// `declare const x: T` diz que algo existe EM OUTRO LUGAR. Não introduz binding
// e não emite nada — é o significado inteiro da palavra-chave.
//
// O motor o baixava como uma declaração comum, que ligava o nome a `undefined`
// no escopo. E como o que se declara é quase sempre um global, o binding
// SOMBREAVA exatamente o valor que ele anunciava: `declare const print` seguido
// de `print(x)` morria com "print is not a function", enquanto a mesma chamada
// uma linha ACIMA da declaração funcionava.
//
// `declare function f` nunca foi afetado — uma função sem corpo já não é nada a
// emitir — e é isso que fazia o defeito parecer um global que só às vezes
// existia.

declare const println: (texto: string) => void;
declare function print(texto: string): void;

describe("declare não introduz binding", () => {
  test("`declare const` não sombreia o global de mesmo nome", () => {
    expect(typeof println).toBe("function");
  });

  test("`declare function` continua sem sombrear", () => {
    expect(typeof print).toBe("function");
  });

  test("e o global chamado através do nome declarado realmente roda", () => {
    let rodou = false;
    try {
      println("");
      rodou = true;
    } catch {
      rodou = false;
    }
    expect(rodou).toBe(true);
  });
});
