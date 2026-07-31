import { describe, test, expect } from "rts:test";

// Um generator LAZY alcançado através de outra função perdia TUDO:
// `it.next()` lia `undefined` em vez do primeiro valor.
//
//   function* inner(o) { const a = yield o.v; yield a * 2; }
//   function wrapper() { const o = {v:5}; return inner(o); }
//   wrapper().next().value       // undefined  ·  Node: 5
//
// Era DUPLO BOXING. Desde #2042 o call site do ctor lazy boxa o handle como word
// `TAG_OBJECT`; mas o fixpoint que propaga a marca de generator para quem apenas
// REPASSA continuava marcando o repassador como `Repr::Int64` (o comentário
// descrevia o estado ANTERIOR àquele fix). O repassador então carregava uma word
// já boxada, e o call site a boxava de novo — destruindo o handle.
//
// Correção nos dois lados: o repassador passa a ser `Tagged` (é o que ele de fato
// carrega), e o call site só boxa quando o retorno é o handle CRU (`Int64`), que
// é o caso exclusivo do ctor.
//
// Valores conferidos contra o Node. Pré-computado no top-level.

function* inner(o) { const a = yield o.v; yield a * 2; }
function wrapper() { const o = { v: 5 }; return inner(o); }
const it = wrapper();
const viaFnPrimeiro = it.next().value;
const viaFnEnviado = it.next(10).value;

function* semParam() { const a = yield 1; yield a + 1; }
function fwd() { return semParam(); }
const i2 = fwd();
const semParamPrimeiro = i2.next().value;
const semParamEnviado = i2.next(9).value;

function* contador(n) { let i = 0; while (i < n) { yield i; i = i + 1; } }
function fwd3() { return contador(3); }
const spreadEncaminhado = [...fwd3()].join(",");
let somaForOf = 0;
for (const x of fwd3()) { somaForOf = somaForOf + x; }

// dois níveis de repasse
function fwdA() { return contador(2); }
function fwdB() { return fwdA(); }
const doisNiveis = [...fwdB()].join(",");

// ── não-regressões ─────────────────────────────────────────────────────────
const d = semParam();
const diretoPrimeiro = d.next().value;
const diretoEnviado = d.next(4).value;
const spreadDireto = [...contador(2)].join(",");

describe("generator lazy encaminhado por outra função", () => {
  test("primeiro next() através da função", () => expect(viaFnPrimeiro).toBe(5));
  test("valor ENVIADO chega ao yield", () => expect(viaFnEnviado).toBe(20));
  test("sem parâmetro: primeiro next()", () => expect(semParamPrimeiro).toBe(1));
  test("sem parâmetro: valor enviado", () => expect(semParamEnviado).toBe(10));
  test("spread do resultado encaminhado", () => expect(spreadEncaminhado).toBe("0,1,2"));
  test("for-of do resultado encaminhado", () => expect(somaForOf).toBe(3));
  test("dois níveis de repasse", () => expect(doisNiveis).toBe("0,1"));
});

describe("não-regressões", () => {
  test("chamada direta: primeiro next()", () => expect(diretoPrimeiro).toBe(1));
  test("chamada direta: valor enviado", () => expect(diretoEnviado).toBe(5));
  test("spread direto", () => expect(spreadDireto).toBe("0,1"));
});
