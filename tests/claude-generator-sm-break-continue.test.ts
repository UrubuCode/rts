import { describe, test, expect } from "rts:test";

// `break`/`continue` MODELADOS na state-machine lazy (antes: corpo inteiro
// inelegivel -> eager-buffer).
//
// O buraco que isto fecha e' de CONTRATO entre os dois caminhos de generator:
//
//   - a state-machine sabe `yield` em posicao de VALOR (`const r = yield x`,
//     que le o argumento de um `next(v)` posterior) mas RECUSAVA break/continue;
//   - o eager-buffer respeita break/continue (mantem o corpo verbatim) mas so'
//     expressa `yield` em posicao de STATEMENT.
//
// Um generator com OS DOIS nao era coberto por nenhum: a SM recusava, o eager
// aceitava e deixava o `Yield` cru chegar ao lowering, que morria em
// "expression raw/unrecognized: Yield". Codigo minificado tem break/continue em
// quase todo laco, e essa combinacao e' a forma comum num bundle real.
//
// A correcao NAO e' relaxar o gate: cada laco fatiado em estados empilha um
// alvo `(break -> estado de saida, continue -> estado que re-testa)` e o salto
// vira uma transicao de estado. Relaxar sem modelar fazia o generator rodar
// PARA SEMPRE, sem erro — ver o historico em claude-generator-break-continue.
//
// Todos os valores conferidos contra o Node. Pre-computado no top-level.

// ── break simples, com yield-como-valor (a combinacao que falhava) ──────────
function* comBreak() {
  let i = 0;
  while (true) {
    if (i >= 2) break;
    const r = yield i;
    i = i + 1;
  }
}
const itBreak = comBreak();
const break0 = itBreak.next().value;
const break1 = itBreak.next(9).value;

// ── continue em while, com yield-como-valor ────────────────────────────────
function* comContinue() {
  let i = 0;
  while (i < 5) {
    i = i + 1;
    if (i % 2 === 0) continue;
    const r = yield i;
  }
}
const itCont = comContinue();
const cont0 = itCont.next().value;
const cont1 = itCont.next(0).value;
const cont2 = itCont.next(0).value;

// ── continue num `for`: precisa RODAR O UPDATE antes de re-testar ──────────
// (o `continue` aponta para um estado dedicado que carrega o update; apontar
// direto para o header seria um laco infinito)
function* forContinue() {
  for (let i = 0; i < 5; i = i + 1) {
    if (i % 2 === 0) continue;
    const r = yield i;
  }
}
const forCont: string = [...forContinue()].join(",");

// ── laco aninhado: break sai do INTERNO, outro break sai do EXTERNO ────────
function* aninhado() {
  for (let i = 0; i < 3; i = i + 1) {
    for (let j = 0; j < 3; j = j + 1) {
      if (j === 2) break;
      const r = yield i * 10 + j;
    }
    if (i === 1) break;
  }
}
const nested: string = [...aninhado()].join(",");

// ── break/continue com ROTULO — LACUNA CONHECIDA, ainda não coberta ────────
//
// `break OUT` / `continue OUT` dentro de um generator ainda NÃO são modelados
// pela state-machine: o corpo do laço vira estados, e um rótulo precisa de uma
// PILHA de alvos (um par estado-de-saída / estado-de-continuação por laço
// aninhado) para saber a qual nível o salto se refere. O `break`/`continue` sem
// rótulo, que é o caso comum e o que aparece em bundle minificado, está coberto
// pelos testes acima e funciona.
//
// Sem isso, o corpo cai no eager-buffer, que mantém o `yield` verbatim, e um
// `yield` em posição de VALOR chega cru ao lowering:
//   in fn `rotulado`: expression raw/unrecognized: Yield(...)
//
// Deixado FORA em vez de marcado como esperado-falhando: um teste vermelho
// permanente vira ruído que se aprende a ignorar. Quando a pilha de rótulos
// entrar, este caso volta:
//
//   function* rotulado() {
//     OUT: for (let i = 0; i < 3; i = i + 1) {
//       for (let j = 0; j < 3; j = j + 1) {
//         if (j === 1) continue OUT;
//         if (i === 2) break OUT;
//         const r = yield i * 10 + j;
//       }
//     }
//   }
//   [...rotulado()].join(",") === "0,10"

// ── nao-regressao: laco SEM break continua lazy e infinito ─────────────────
function* infinito() {
  let i = 0;
  while (true) {
    const r = yield i;
    i = i + 1;
  }
}
const inf = infinito();
const inf0 = inf.next().value;
const inf1 = inf.next(0).value;
const inf2 = inf.next(0).value;

describe("break/continue como alvo de estado na SM", () => {
  test("break com yield-como-valor termina e entrega o valor enviado", () => {
    expect(break0).toBe(0);
    expect(break1).toBe(1);
  });

  test("continue em while pula a iteracao", () => {
    expect(cont0 + "," + cont1 + "," + cont2).toBe("1,3,5");
  });

  test("continue em for roda o update antes de re-testar", () => {
    expect(forCont).toBe("1,3");
  });

  test("laco aninhado: break interno e break externo", () => {
    expect(nested).toBe("0,1,10,11");
  });

  test("laco sem break continua lazy (nao materializa)", () => {
    expect(inf0 + "," + inf1 + "," + inf2).toBe("0,1,2");
  });
});
