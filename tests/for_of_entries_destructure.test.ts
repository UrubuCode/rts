import { describe, test, expect } from "rts:test";

// Regression (cross-runtime #208/#494): `for (const [i, v] of arr.entries())`
// onde o array eh de strings imprimia o handle CRU do valor
// (`0:281474976710658`) em vez do conteudo (`0:a`). Causa: o destructuring
// no for-of so' marcava slot 0 de Object.entries como Handle; o slot 1
// (valor) de arr.entries() ficava i64 nao-marcado, e o template imprimia
// o numero do handle. Fix: marca slots sem tipo Handle estatico como
// vec-slot ambiguo (var_vec_slot_values/var_member_call_values) pra que a
// coercao template detecte string/handle em runtime.

let out = "";
function print(v: string): void { out += v + "\n"; }

// arr.entries() de strings — value (slot 1) eh string handle
const arr = ["a", "b", "c"];
for (const [i, v] of arr.entries()) { out += i + ":" + v + " "; }
out += "\n";

// pares numericos continuam OK
for (const [i, v] of [[10, 20], [30, 40]]) { out += i + "+" + v + " "; }
out += "\n";

// Object.entries (slot 0 = key string) continua OK
for (const [k, val] of Object.entries({ x: 1, y: 2 })) { out += k + "=" + val + " "; }
out += "\n";

// entries de numeros
const nums = [100, 200];
for (const [i, n] of nums.entries()) { out += i + "@" + n + " "; }

describe("for-of entries destructure", () => {
  test("value string nao vira handle cru", () =>
    expect(out).toBe("0:a 1:b 2:c \n10+20 30+40 \nx=1 y=2 \n0@100 1@200 "));
});
