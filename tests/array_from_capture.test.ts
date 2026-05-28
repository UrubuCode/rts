import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `Array.from(src, mapper)` onde o mapper captura
// uma var LOCAL (ex: const factor) dava lixo — o fn_ptr nu nao carregava os
// bound_args, entao a captura recebia o indice. Fix: roteia ao caminho BOUND
// (PARALLEL_MAP_BOUND + REIFY_CAPTURED) quando o mapper eh `__lifted_cap_*`.
// CONTRASTE: `arr.map(x => x * factor)` com captura local ja' funcionava.

let out = "";
function print(v: string): void { out += v + "\n"; }

// {length:n} com captura local
function build(): number[] {
  const factor = 2;
  return Array.from({ length: 4 }, (_, i) => i * factor);
}
print(build().join(","));            // 0,2,4,6

// string source com captura local
function fromStr(): string[] {
  const suffix = "!";
  return Array.from("abc", (c) => c + suffix);
}
print(fromStr().join(","));          // a!,b!,c!

// array source com captura local
function fromArr(): number[] {
  const base = 100;
  return Array.from([1, 2, 3], (x) => x + base);
}
print(fromArr().join(","));          // 101,102,103

// captura de param de fn (nao so' const local)
function withParam(mult: number): number[] {
  return Array.from({ length: 3 }, (_, i) => i * mult);
}
print(withParam(10).join(","));      // 0,10,20

describe("Array.from captura local", () => {
  test("mapper capturando local nao vira lixo", () =>
    expect(out).toBe("0,2,4,6\na!,b!,c!\n101,102,103\n0,10,20\n"));
});
