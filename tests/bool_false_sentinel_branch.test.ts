import { describe, test, expect } from "rts:test";

// (cross-runtime #368) Campo bool `false` lido de um objeto via member access
// ambiguo (sem field type estatico — ex: retorno de fn) era empacotado pelo
// codegen como sentinel i64::MIN. console.log/INSPECT o reconhecia ("false"),
// mas to_branch_cond (if/while/!) testava apenas `!= 0`, e i64::MIN != 0 ->
// tratava `false` como TRUTHY. Padrao classico de iterador quebrava:
// `const {done} = it.next(); if (done) break;` nunca parava corretamente.
//
// Fix: to_branch_cond trata i64::MIN (BOOL_FALSE) como falsy, simetrico ao
// sentinel undefined (i64::MIN+2) ja' tratado. BOOL_TRUE (i64::MIN+1) segue
// truthy.

function mkFalse(): string {
  const o = { done: false };
  const f = (() => o)(); // forca o retorno via fn (member ambiguo)
  return f["done"] ? "truthy" : "falsy";
}

function mkTrue(): string {
  const o = { done: true };
  const f = (() => o)();
  return f["done"] ? "truthy" : "falsy";
}

// padrao de iterador: const {done} = next(); loop ate done
function iterate(): string {
  const it = { _i: 0, next() { this._i++; return { value: this._i, done: this._i > 3 }; } };
  const out: number[] = [];
  for (let i = 0; i < 10; i++) {
    const { value, done } = it.next();
    if (done) break;
    out.push(value);
  }
  return out.join(",");
}

// negacao de campo bool ambiguo
function negate(): string {
  const o = { ready: false };
  const g = (() => o)();
  return !g["ready"] ? "not-ready" : "ready";
}

// Comparacao estrita/loose com bool literal sobre campo bool ambiguo.
function eqFalse(): string {
  const o = { done: false };
  const m = (() => o)();
  return m["done"] === false ? "eq" : "neq";
}
function eqTrue(): string {
  const o = { done: true };
  const m = (() => o)();
  return m["done"] === true ? "eq" : "neq";
}
// JS-spec: numero real nao eh === a bool (tipos diferentes), mas == coage.
const strictNumBool = (1 === (true as any)) ? "T" : "F";
const looseNumBool = (1 == (true as any)) ? "T" : "F";

const a = mkFalse();
const b = mkTrue();
const c = iterate();
const d = negate();
const e = eqFalse();
const f = eqTrue();

describe("bool false sentinel in branch (#368)", () => {
  test("false field is falsy in if", () => expect(a).toBe("falsy"));
  test("true field is truthy in if", () => expect(b).toBe("truthy"));
  test("iterator done=false loops, done=true breaks", () => expect(c).toBe("1,2,3"));
  test("negation of false field", () => expect(d).toBe("not-ready"));
  test("field === false (strict)", () => expect(e).toBe("eq"));
  test("field === true (strict)", () => expect(f).toBe("eq"));
  test("1 === true is false (JS strict)", () => expect(strictNumBool).toBe("F"));
  test("1 == true is true (JS loose)", () => expect(looseNumBool).toBe("T"));
});
