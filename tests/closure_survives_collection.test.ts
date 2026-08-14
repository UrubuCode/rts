import { describe, test, expect } from "rts:test";

// `closure_new` aloca a célula do callable, marca ao lado dela o que a torna
// chamável, e SÓ ENTÃO aloca o objeto `prototype` que toda função tem. Entre as
// duas alocações, a única coisa que nomeia a primeira célula é um local do Rust
// com um índice CRU — e `roots::scan_stack` só guarda palavras que são
// referências codificadas sem ambiguidade, o que um índice não é. Então a
// segunda alocação podia coletar a closure que a pediu.
//
// O sintoma era `TypeError: object is not a function` depois de algumas
// centenas de milhares de closures: a célula era varrida, a entrada `callables`
// ia com ela, e o valor devolvido nomeava o que quer que tivesse ficado com o
// índice. Três linhas do `bench/analytic.ts` morriam assim — `closure
// make+call`, `array map 16` e `array filter 16` — e QUAL delas morria mudava
// com o binário, porque depende de quando o coletor corre.
//
// As contagens aqui são medidas e não escolhidas: a região tem 65 536 células e
// uma closure ocupa duas, então uma coleta cai a cada ~32 000. Menos do que isso
// e o teste passa sem a correção, o que faria dele um teste de nada.

describe("uma closure sobrevive à coleta que ela própria dispara", () => {
  test("uma closure nova por iteração continua chamável", () => {
    let n = 0;
    for (let i = 0; i < 200000; i++) {
      const f = (x: number): number => x + 1;
      n = n + f(1);
    }
    expect(n).toBe(400000);
  });

  test("uma closure passada a um método que aloca continua chamável", () => {
    // A forma que o bench escrevia: a callback nasce como argumento, e o
    // método constrói um array — que aloca, e portanto pode coletar.
    const xs = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    let a = 0;
    for (let i = 0; i < 100000; i++) a += xs.map((x: number) => x + 1)[0];
    expect(a).toBe(200000);
  });

  test("o `prototype` que a closure recebe também sobrevive", () => {
    // A mesma armadilha uma linha depois: até a escrita aterrar, o objeto
    // `prototype` é nomeado só por um local, e `put` pode precisar de um bloco
    // de overflow — que aloca.
    let ok = 0;
    for (let i = 0; i < 100000; i++) {
      const f = function (x: number): number { return x; };
      if (typeof (f as any).prototype === "object") ok = ok + 1;
    }
    expect(ok).toBe(100000);
  });
});
