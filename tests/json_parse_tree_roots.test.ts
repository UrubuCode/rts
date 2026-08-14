import { describe, test, expect } from "rts:test";

// `JSON.parse` materializa uma árvore alocando a CADA nó, e cada nó já
// construído ficava num `Vec` no monte do Rust até o pai o guardar — invisível
// à coleta que o nó seguinte dispara. Duas exposições por composto: o laço que
// constrói os filhos, e a alocação do próprio array ou objeto que os recebe.
//
// Confere o CONTEÚDO em profundidade, porque a falha desta classe é uma
// resposta errada. Com as raízes desligadas por um interruptor temporário este
// programa respondeu errado em três de três execuções; com elas, zero.

describe("a árvore que JSON.parse constrói sobrevive a si própria", () => {
  test("um documento aninhado volta inteiro, muitas vezes seguidas", () => {
    const src = JSON.stringify({
      a: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
      b: { c: "ccc", d: [{ e: 1 }, { e: 2 }, { e: 3 }] },
      f: "fff",
      g: ["h", "i", "j", "k"],
      l: { m: { n: { o: 42 } } },
    });
    let bad = 0;
    for (let r = 0; r < 3000; r++) {
      const o: any = JSON.parse(src);
      if (o.a.length !== 10 || o.a[9] !== 10) bad = bad + 1;
      else if (o.b.c !== "ccc" || o.b.d[2].e !== 3) bad = bad + 1;
      else if (o.f !== "fff" || o.g[3] !== "k") bad = bad + 1;
      else if (o.l.m.n.o !== 42) bad = bad + 1;
    }
    expect(bad).toBe(0);
  });
});
