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
import { backends, supports, threads } from "rts:rigid";

// ── quantos motores este build tem ─────────────────────────────────────────
//
// Um, hoje: o solver gather. O teste não afirma que é UM — afirma que é pelo
// menos um e que o número é real, porque acrescentar a Rapier atrás de uma
// feature deve fazer este teste continuar passando em vez de virar manutenção.
const n = backends();
if (typeof n !== "number") throw new Error("backends() deve responder um número");
if (n < 1) throw new Error("um build sem backend nenhum não deveria instalar rts:rigid");

// ── a capacidade que ESTE build tem ────────────────────────────────────────
//
// 0 é "existe algum backend", e tem de ser 1: se não houvesse, `step` não teria
// o que chamar e a superfície seria um nome oco.
if (supports(0) !== 1) throw new Error("nenhum backend disponível para uma cena comum");

// ── e as quatro que ele NÃO tem, uma a uma ────────────────────────────────
//
// Cada uma destas responde 0 hoje, e cada 0 é um fato medido ou declarado, não
// uma função pela metade:
//
//   1 casca contra casca  docs/colisores.md §3 mediu 2,2 BILHÕES de produtos
//                         escalares por frame a 2000 corpos. O solver degrada o
//                         par para esfera de propósito.
//   2 colisão contínua    não há teste varrido; o teto de velocidade é uma rede,
//                         não um conserto.
//   3 angular             não há torque nem velocidade angular em lugar nenhum.
//   4 juntas              não há junta nem articulação de onde pendurar uma.
//
// O dia em que a Rapier entrar atrás de uma feature, este bloco passa a falhar
// NAQUELE build — e falhar é o comportamento certo, porque a resposta terá
// mudado de verdade. É por isso que ele checa contra `backends()`: com mais de
// um motor presente, a afirmação deixa de ser sobre este build.
if (n === 1) {
  const faltas = [1, 2, 3, 4];
  for (let i = 0; i < faltas.length; i++) {
    const r = supports(faltas[i]);
    if (r !== 0) {
      throw new Error(
        "com só o solver gather, supports(" + faltas[i] + ") deveria ser 0 e veio " + r,
      );
    }
  }
}

// ── uma necessidade que não existe responde 0, nunca 1 ────────────────────
//
// A direção conservadora, e ela importa: um programa perguntando por algo que
// este build nunca ouviu falar tem de ser respondido "não" em vez de "sim por
// omissão". Um `supports` otimista é exatamente a fronteira que mente.
if (supports(99) !== 0) throw new Error("uma necessidade desconhecida deve responder 0");
if (supports(0 - 1) !== 0) throw new Error("um código negativo deve responder 0");

// ── e o que já existia continua ───────────────────────────────────────────
const t = threads();
if (typeof t !== "number" || t < 1) throw new Error("threads() deve responder ao menos 1");

console.log("ok: backends=" + n + " threads=" + t + " suporta-cena-comum=" + supports(0));
console.log("faltam: casca-x-casca=" + supports(1) + " continua=" + supports(2) +
            " angular=" + supports(3) + " juntas=" + supports(4));
