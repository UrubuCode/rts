import { describe, test, expect } from "rts:test";

// Uma local inferida `boolean` (inicializada com `true`/`false`) que depois
// recebe um valor que o front não prova booleano fazia o coerce da atribuição
// pedir `Repr::Bool` — coerção que NÃO EXISTE — e a função inteira falhava com
// `cannot coerce Tagged to Bool` / `cannot coerce Int64 to Bool`. Medido num
// `__rtsn_ctor_*` de um bundle real do WhatsApp Web; é a forma ordinária de JS
// minificado, onde nada é anotado e toda flag nasce como literal `false`.
//
// A saída ERRADA aqui seria aplicar `ToBoolean`: JS não converte na atribuição.
// Depois de `b = o.x` com `o.x === 1`, o Node tem `b === 1` e
// `typeof b === "number"` — converter imprimiria `true`. O conserto é
// representacional: a local nunca foi provadamente booleana, então liga
// `Tagged` desde o início (`join(Bool, Tagged) = Tagged`). Uma local só
// atribuída com booleanos de verdade continua no slot `Bool` nativo.
//
// O espelho do mesmo erro está na ANOTAÇÃO: `let b: boolean = x` com `x` não-bool
// — o tipo TS é apagado, então a anotação é fronteira não-confiável, não prova.
//
// Valores conferidos contra o Node (v22). Pré-computado no top-level.

const o: any = { x: 1, s: "abc", z: 0 };

// ── 1. local inferida bool recebendo número/string/objeto ───────────────────
function localInferida() {
  let b = false;
  b = o.x;
  return "" + b + " " + typeof b;
}

function localInferidaStr() {
  let b = true;
  b = o.s;
  return "" + b + " " + typeof b;
}

// ── 2. a local que SÓ recebe booleanos mantém a identidade booleana ─────────
function soBooleanos() {
  let b = false;
  b = o.x > 0;
  const antes = "" + b + " " + typeof b;
  b = !b;
  return antes + " " + b;
}

// ── 3. anotação `boolean` sobre valor não-bool: o valor manda ───────────────
function anotacaoNaoConverte() {
  const n: any = 1;
  let b: boolean = n;
  return "" + b + " " + typeof b;
}

// ── 4. dentro de construtor (onde o bundle real quebrou) ────────────────────
class Ctor {
  r: string;
  constructor(src: any) {
    let flag = false;
    flag = src.x;
    this.r = "" + flag + " " + typeof flag;
  }
}

// ── 5. valor falsy não vira `false`: `0` continua `0` ───────────────────────
function falsyContinuaNumero() {
  let b = true;
  b = o.z;
  return "" + b + " " + typeof b;
}

const r1 = localInferida();
const r2 = localInferidaStr();
const r3 = soBooleanos();
const r4 = anotacaoNaoConverte();
const r5 = new Ctor(o).r;
const r6 = falsyContinuaNumero();

describe("local boolean recebendo não-boolean", () => {
  test("número atribuído a local bool não vira true", () => {
    expect(r1).toBe("1 number");
  });
  test("string atribuída a local bool não vira true", () => {
    expect(r2).toBe("abc string");
  });
  test("só booleanos mantém typeof boolean", () => {
    expect(r3).toBe("true boolean false");
  });
  test("anotação boolean não converte", () => {
    expect(r4).toBe("1 number");
  });
  test("mesma forma dentro de construtor", () => {
    expect(r5).toBe("1 number");
  });
  test("zero atribuído a local bool continua 0", () => {
    expect(r6).toBe("0 number");
  });
});
