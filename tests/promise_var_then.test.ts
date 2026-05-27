import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `const p = Promise.resolve(x); p.then(cb)`
// crashava SIGILL (var nao marcada como Promise -> p.then caia no fallback
// Map -> trapz). Fix: decls marca var como Promise quando init eh
// Promise.resolve/reject/all/race/allSettled/any. Aqui validamos que a
// COMPILACAO+EXECUCAO nao crasha (o timing do callback async nao eh
// deterministico no harness, entao checamos so' que o programa roda ate o
// fim e produz o marcador sincrono).

let out = "";
function print(v: string): void { out += v + "\n"; }

const p = Promise.resolve("hello");
p.then(x => print("s=" + x));

const n = Promise.resolve(42);
n.then(x => print("n=" + x));

const a = Promise.all([1, 2, 3]);
const b = Promise.race([1, 2]);
print("sync-end");   // marcador sincrono — chega aqui = nao crashou

describe("promise var then", () => {
  test("var promise + then nao crasha", () =>
    expect(out.includes("sync-end")).toBe(true));
});
