// `rts:rigid` diz o que NÃO pode fazer, e é isso que está sob teste.
//
//   rts.exe test tests/claude-rigid-backends.test.ts
//
// ── POR QUE UM TESTE SOBRE RECUSAS ─────────────────────────────────────────
//
// O crate ganhou uma fronteira de backend para que Rapier, Parry, PhysX ou Jolt
// possam entrar. Com UM backend só, essa fronteira ainda é uma hipótese — não há
// como provar que ela acomoda um segundo motor até um segundo motor existir.
//
// O que JÁ é provável, e é a metade que mais importa, é o caminho da RECUSA.
// Uma fronteira de plug-in que responde "sim" por omissão é pior que nenhuma:
// ela produz um programa que roda numa máquina onde o motor certo está presente
// e roda ERRADO onde não está, com os mesmos números plausíveis nos dois casos.
//
// Então o que este arquivo pina é que perguntar funciona e que a resposta é a
// verdade sobre este build — inclusive quando a verdade é "não".
import { describe, test, expect } from "rts:test";
import { backends, supports, threads } from "rts:rigid";

describe("rts:rigid diz o que este build NAO pode fazer", () => {
  test("ha ao menos um backend, e o numero e real", () => {
    const n = backends();
    expect(typeof n).toBe("number");
    // Ao menos um, e nao exatamente um: acrescentar a Rapier atras de uma
    // feature deve fazer este teste continuar passando em vez de virar
    // manutencao.
    expect(n >= 1).toBe(true);
  });

  test("uma cena comum tem onde rodar", () => {
    // Se nao houvesse, `step` nao teria o que chamar e a superficie seria um
    // nome oco — que e o que a regra do workspace recusa publicar.
    expect(supports(0)).toBe(1);
  });

  test("as quatro coisas que o solver gather NAO faz respondem 0", () => {
    // Cada zero e um fato medido ou declarado:
    //   1 casca x casca  2,2 BILHOES de produtos escalares/frame a 2000 corpos
    //                    (docs/colisores.md §3). Degradado para esfera, de proposito.
    //   2 continua       nao ha teste varrido; o teto de velocidade e rede.
    //   3 angular        nao ha torque nem velocidade angular.
    //   4 juntas         nenhuma, e nenhuma articulacao de onde pendurar uma.
    //
    // Condicionado a `backends() === 1` para que o dia em que a Rapier entrar
    // este teste continue valido naquele build: a resposta tera mudado de
    // verdade, e nao por manutencao.
    if (backends() === 1) {
      expect(supports(1)).toBe(0);
      expect(supports(2)).toBe(0);
      expect(supports(3)).toBe(0);
      expect(supports(4)).toBe(0);
    }
  });

  test("uma necessidade desconhecida responde 0, nunca 1", () => {
    // A direcao conservadora. Um `supports` otimista e exatamente a fronteira
    // que mente: um programa perguntando por algo que este build nunca ouviu
    // falar tem de ser respondido "nao" em vez de "sim por omissao".
    expect(supports(99)).toBe(0);
    expect(supports(-1)).toBe(0);
  });

  test("threads() continua respondendo", () => {
    const t = threads();
    expect(typeof t).toBe("number");
    expect(t >= 1).toBe(true);
  });
});
