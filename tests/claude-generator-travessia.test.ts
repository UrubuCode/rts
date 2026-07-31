import { describe, test, expect } from "rts:test";

// O iterador de um generator tem de manter o protocolo (`next`/`return`/`throw`)
// ao atravessar o RETORNO de outra função.
//
// Antes só funcionava quando o motor rastreava estaticamente que o valor veio de
// uma chamada de generator: `generator_receiver_handle` (stmt.rs) reconhecia um
// `Ident` marcado em `generator_locals` ou uma `Call` direta que `gen_call_kind`
// resolvia via `sigs`. Uma função que apenas REPASSA (`function h(){ return g() }`)
// não estava em `sigs` como generator, então `h().next()` chegava ao despacho
// como `Number.next` e bailava.
//
// Duas peças fizeram isso funcionar, e as duas importam:
//   1. um FIXPOINT propaga a marca de generator para quem só repassa o resultado
//      (module_jit) — e mantém a REPR do alvo, porque um generator lazy é um
//      handle cru e coagi-lo perde o handle;
//   2. o despacho dinâmico reconhece a `Entry::GenState` pelo HANDLE
//      (dyndispatch), então o protocolo não depende só do rastreio estático.
//
// LIMITE CONHECIDO, honesto: atravessar um ARRAY (`const a = [g()]; a[0].next()`)
// ainda perde o protocolo — o handle é re-boxado ao entrar no array e vira outro.
// Fica na issue #2042; este teste cobre o que passou a funcionar.
//
// Valores conferidos contra o Node. Pré-computado no top-level.

// ── repasse simples ────────────────────────────────────────────────────────
function* gen1() { yield 1; yield 2; }
function repassa() { return gen1(); }
const doRepasse = repassa().next().value;

// ── repasse consumindo dois valores ────────────────────────────────────────
function* gen2() { yield 10; yield 20; }
function repassa2() { return gen2(); }
const it2 = repassa2();
const dois = it2.next().value + it2.next().value;

// ── repasse de generator ANINHADO ──────────────────────────────────────────
function externa(): number {
  function* g() { yield 7; }
  function h() { return g(); }
  return h().next().value;
}
const aninhadoRepasse = externa();

// ── chamada direta não pode regredir ───────────────────────────────────────
function* direto() { yield 5; }
const semRepasse = direto().next().value;

// ── local nomeado não pode regredir ────────────────────────────────────────
function* gen3() { yield 3; }
const itLocal = gen3();
const viaLocal = itLocal.next().value;

describe("iterador atravessa o retorno de função", () => {
  test("repasse simples mantém o protocolo", () => {
    expect(doRepasse).toBe(1);
  });

  test("consome dois valores em ordem pelo repasse", () => {
    expect(dois).toBe(30);
  });

  test("repasse de generator aninhado", () => {
    expect(aninhadoRepasse).toBe(7);
  });

  test("chamada direta não regrediu", () => {
    expect(semRepasse).toBe(5);
  });

  test("local nomeado não regrediu", () => {
    expect(viaLocal).toBe(3);
  });
});
