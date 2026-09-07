import { describe, test, expect } from "rts:test";

// O que este ficheiro fixa: um literal de texto vale o mesmo em todo o sítio
// onde é escrito, e continua a valer o mesmo depois de um `yield`, dentro de um
// fecho, e num ramo que a primeira passagem não tomou.
//
// Existe porque materializar um literal é uma CHAMADA e não uma constante, e
// essa chamada passou a ser feita uma vez por corpo em vez de uma vez por
// escrita. As respostas abaixo não podem mudar com isso, e a mais delicada é a
// do gerador: um corpo que suspende é reescrito à volta de cada suspensão, e um
// valor definido à entrada e lido depois de um `yield` não é o valor que era.
//
// Passa igualmente no binário anterior à mudança, que é o que um teste de
// semântica tem de fazer.

describe("um literal escrito muitas vezes", () => {
  test("vale o mesmo em cada passagem de um laço", () => {
    let s = "";
    for (let i = 0; i < 4; i++) {
      s = s + "ab";
    }
    expect(s).toBe("abababab");
    expect(s.length).toBe(8);
  });

  test("é igual a si mesmo escrito noutro sítio", () => {
    const primeiro = "mesmo";
    let segundo = "";
    for (let i = 0; i < 1; i++) {
      segundo = "mesmo";
    }
    expect(primeiro === segundo).toBe(true);
  });

  test("dois literais diferentes continuam diferentes", () => {
    let a = "";
    let b = "";
    for (let i = 0; i < 2; i++) {
      a = a + "x";
      b = b + "y";
    }
    expect(a).toBe("xx");
    expect(b).toBe("yy");
  });

  test("o vazio é um literal como outro qualquer", () => {
    let s = "z";
    for (let i = 0; i < 3; i++) {
      s = s + "";
    }
    expect(s).toBe("z");
    expect(s.length).toBe(1);
  });
});

describe("um literal num ramo que a primeira passagem não toma", () => {
  test("responde certo quando o ramo é finalmente tomado", () => {
    const visto: string[] = [];
    for (let i = 0; i < 4; i++) {
      if (i === 3) {
        visto.push("tarde");
      } else {
        visto.push("cedo");
      }
    }
    expect(visto.join(",")).toBe("cedo,cedo,cedo,tarde");
  });

  test("e num ramo que nunca é tomado não estraga o outro", () => {
    let s = "";
    for (let i = 0; i < 3; i++) {
      if (i > 100) {
        s = s + "nunca";
      } else {
        s = s + "sempre";
      }
    }
    expect(s).toBe("sempresempresempre");
  });
});

describe("um literal atravessando uma fronteira de função", () => {
  test("um fecho tem o seu próprio, e vê o de fora", () => {
    const fora = "fora";
    const dentro = (): string => "dentro" + fora;
    expect(dentro()).toBe("dentrofora");
    expect(dentro()).toBe("dentrofora");
  });

  test("uma função chamada muitas vezes responde o mesmo", () => {
    function etiqueta(n: number): string {
      return "n=" + n;
    }
    expect(etiqueta(1)).toBe("n=1");
    expect(etiqueta(2)).toBe("n=2");
    expect(etiqueta(1)).toBe("n=1");
  });
});

describe("um literal depois de uma suspensão", () => {
  // O caso que o içamento tem de recusar. Um corpo que suspende é reescrito à
  // volta de cada `yield`, então nada aqui pode ser içado para a entrada.
  test("um gerador devolve o mesmo texto em cada passo", () => {
    function* passos(): Generator<string, void, unknown> {
      yield "um";
      yield "dois";
      yield "um";
    }
    const saída: string[] = [];
    for (const p of passos()) {
      saída.push(p);
    }
    expect(saída.join("-")).toBe("um-dois-um");
  });

  test("um gerador num laço junta o literal ao que recebeu", () => {
    function* juntos(): Generator<string, void, unknown> {
      for (let i = 0; i < 3; i++) {
        yield "i" + i;
      }
    }
    const saída: string[] = [];
    for (const p of juntos()) {
      saída.push(p);
    }
    expect(saída.join(",")).toBe("i0,i1,i2");
  });
});
