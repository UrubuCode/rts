import { describe, test, expect } from "rts:test";

// Seis nativos constroem uma lista de valores alocando a CADA passo — internar
// uma string aloca — e até a lista aterrar onde o coletor a alcança ela é
// nomeada só por um `Vec` no monte do Rust, que nenhuma varredura nossa lê.
//
// Este ficheiro confere o CONTEÚDO e não o comprimento, porque a falha desta
// classe é uma resposta errada e não um crash. Com as raízes desligadas por um
// interruptor temporário, este programa respondeu errado em três de três
// execuções; com elas, zero. O interruptor foi apagado depois de responder.
//
// O que isto NÃO diz: qual dos seis sítios falhou. Diz que a classe é real e
// que estes seis têm a mesma forma.

describe("listas que um nativo constrói sobrevivem às suas próprias alocações", () => {
  test("Object.keys e o spread de objeto", () => {
    let bad = 0;
    for (let r = 0; r < 300; r++) {
      const o: any = {};
      for (let i = 0; i < 40; i++) o["k" + i] = i;
      const ks: any = Object.keys(o);
      if (ks.length !== 40 || ks[39] !== "k39" || ks[0] !== "k0") bad = bad + 1;
      const cp: any = { ...o };
      if (cp["k39"] !== 39 || cp["k0"] !== 0) bad = bad + 1;
    }
    expect(bad).toBe(0);
  });

  test("o spread de uma string interna um ponto de código de cada vez", () => {
    let bad = 0;
    for (let r = 0; r < 600; r++) {
      const cs: any = [..."abcdefghijklmnopqrstuvwxyz"];
      if (cs.length !== 26 || cs[0] !== "a" || cs[25] !== "z") bad = bad + 1;
    }
    expect(bad).toBe(0);
  });

  test("split interna uma peça de cada vez, pelos dois caminhos", () => {
    let bad = 0;
    for (let r = 0; r < 600; r++) {
      const ps: any = "a,b,c,d,e,f,g,h,i,j,k,l,m,n,o,p".split(",");
      if (ps.length !== 16 || ps[0] !== "a" || ps[15] !== "p") bad = bad + 1;
      // Separador vazio: o caminho geral em `pattern.rs`, não o estreito.
      const cs: any = "abcdef".split("");
      if (cs.length !== 6 || cs[5] !== "f") bad = bad + 1;
    }
    expect(bad).toBe(0);
  });

  test("os grupos de uma expressão regular são internados um a um", () => {
    const re = /(\w)(\w)(\w)/;
    let bad = 0;
    for (let r = 0; r < 600; r++) {
      const m: any = re.exec("xyz");
      if (m === null || m[1] !== "x" || m[3] !== "z") bad = bad + 1;
    }
    expect(bad).toBe(0);
  });
});
