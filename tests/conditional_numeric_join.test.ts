import { describe, test, expect } from "rts:test";

// O que este ficheiro fixa: o VALOR de `?:`, `&&` e `||` quando os dois lados
// produzem números, e a verdade desse valor.
//
// Existe porque a junção desses três operadores passou a carregar a
// representação que os dois caminhos concordam em produzir, em vez de ser
// sempre a forma genérica. Isso é uma decisão de máquina e não pode mudar
// nenhuma das respostas abaixo — em particular a de `NaN`, que é o número
// falsy e o único caso em que "é um double" e "é verdadeiro" divergem.
//
// Passa igualmente no binário anterior à mudança, que é o que um teste de
// semântica tem de fazer: ele afirma o que a LINGUAGEM significa, não o que
// esta emissão faz.

function pick(c: boolean, x: number, y: number): number {
  return c ? x : y;
}

describe("o valor de uma condicional entre dois números", () => {
  test("é o operando do lado escolhido", () => {
    expect(pick(true, 1, 2)).toBe(1);
    expect(pick(false, 1, 2)).toBe(2);
  });

  test("continua a somar como número", () => {
    let total = 0;
    for (let i = 0; i < 5; i++) {
      total = total + (i < 3 ? 10 : 20);
    }
    expect(total).toBe(70);
  });

  test("preserva a diferença entre 0 e -0", () => {
    expect(Object.is(pick(true, -0, 1), -0)).toBe(true);
    expect(Object.is(pick(true, 0, 1), -0)).toBe(false);
  });

  test("preserva NaN, que não é igual a si mesmo", () => {
    const v = pick(true, NaN, 1);
    expect(v === v).toBe(false);
    expect(Number.isNaN(v)).toBe(true);
  });
});

describe("a verdade de uma condicional entre dois números", () => {
  test("zero e menos zero são falsos", () => {
    expect(pick(true, 0, 1) ? "t" : "f").toBe("f");
    expect(pick(true, -0, 1) ? "t" : "f").toBe("f");
  });

  test("NaN é falso, que é o caso que uma comparação com zero erra", () => {
    expect(pick(true, NaN, 1) ? "t" : "f").toBe("f");
    expect(pick(false, 1, NaN) ? "t" : "f").toBe("f");
  });

  test("qualquer outro número é verdadeiro", () => {
    expect(pick(true, 1, 0) ? "t" : "f").toBe("t");
    expect(pick(true, -1, 0) ? "t" : "f").toBe("t");
    expect(pick(true, Infinity, 0) ? "t" : "f").toBe("t");
  });
});

describe("os curtos-circuitos passam pela mesma junção", () => {
  test("`||` responde o primeiro operando verdadeiro", () => {
    expect(0 || 7).toBe(7);
    expect(3 || 7).toBe(3);
  });

  test("`&&` responde o primeiro falso, ou o último", () => {
    expect(0 && 7).toBe(0);
    expect(3 && 7).toBe(7);
  });

  test("NaN corta um `&&` e não corta um `||` para um valor verdadeiro", () => {
    expect(Number.isNaN(NaN && 7)).toBe(true);
    expect(NaN || 7).toBe(7);
  });

  test("o resultado ainda é um número que soma", () => {
    const v: number = (0 || 5) + (3 && 4);
    expect(v).toBe(9);
  });
});

describe("uma condicional cujos lados discordam continua genérica", () => {
  test("um número e uma string sobrevivem os dois", () => {
    const c = true;
    expect(c ? 1 : "a").toBe(1);
    expect(!c ? 1 : "a").toBe("a");
  });

  test("e o resultado ainda concatena quando é texto", () => {
    const c = false;
    const v = (c ? 1 : "a") + "b";
    expect(v).toBe("ab");
  });
});
