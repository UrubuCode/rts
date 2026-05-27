import { describe, test, expect } from "rts:test";

// Regression (cross-runtime #316): `new Set(["a","b","a"])` nao deduplicava
// chaves string (size 3 em vez de 2) e has("a") retornava false. Causa: o
// constructor coergia o ELEMENTO string (handle) para i64 e usava o numero
// do handle como key — cada "a" tinha handle distinto. Fix: usar o conteudo
// da string (proprio handle) como key. (numericos ja' deduplicavam OK)

let out = "";
function print(v: string): void { out += v + "\n"; }

const s = new Set(["a", "b", "a", "c", "b"]);
print(s.size + "");                    // 3
print([...s].join(","));               // a,b,c
print(s.has("a") + " " + s.has("z"));  // true false

// numerico continua OK
const n = new Set([1, 1, 2, 3, 3]);
print(n.size + "");                    // 3
print([...n].join(","));               // 1,2,3

describe("set string dedup", () => {
  test("dedup + has por conteudo", () =>
    expect(out).toBe("3\na,b,c\ntrue false\n3\n1,2,3\n"));
});
