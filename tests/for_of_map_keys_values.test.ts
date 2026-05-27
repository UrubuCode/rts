import { describe, test, expect } from "rts:test";

// Regression (cross-runtime #222): `for (const k of map.keys())` /
// `map.values()` com chaves/valores STRING imprimia o handle cru
// (`562949953421320`) em vez do conteudo. Causa: bind simples de for-of sem
// tipo Handle estatico ficava i64 nao-marcado; o slot (handle string) nao
// era coerido no template. `[...map.keys()].join()` funcionava (spread coage
// vec-slot em runtime). Fix: marca o elem do for-of como vec-slot ambiguo
// quando o bind nao tem tipo Handle/F64/I32 estatico.

let out = "";
function print(v: string): void { out += v + "\n"; }

const m = new Map<string, string>();
m.set("x", "1"); m.set("y", "2"); m.set("z", "3");

// for-of keys
let ks = "";
for (const k of m.keys()) ks += k;
print(ks);                          // xyz

// for-of values
let vs = "";
for (const v of m.values()) vs += v;
print(vs);                          // 123

// spread continua OK
print([...m.keys()].join("-"));     // x-y-z

// for-of sobre array de strings tambem
let ss = "";
for (const s of ["a", "b", "c"]) ss += s;
print(ss);                          // abc

// array de numeros NAO deve quebrar (regressao guard)
let sum = 0;
for (const n of [10, 20, 30]) sum += n;
print(sum + "");                    // 60

describe("for-of map keys/values", () => {
  test("string keys/values coage no template", () =>
    expect(out).toBe("xyz\n123\nx-y-z\nabc\n60\n"));
});
