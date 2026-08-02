import { describe, test, expect } from "rts:test";

// Um número JS é f64. Um `let s = 0` liga como Int64 (o literal prova inteiro), e
// o `+=` coage o resultado de volta ao repr do local — então acumular um valor
// fracionário nele TRUNCA, silenciosamente. O pre-scan `floatscan` promove o local
// a Float64 quando o valor atribuído exige float.
//
// Estes testes fixam os dois caminhos pelos quais o valor CHEGA sem aparecer na
// árvore do RHS: por uma LIGAÇÃO intermediária, e por baixo de um CAST `as number`.
// Medido antes da correção: `0 + 0.5 + 1 + 1.5` dava 2 em vez de 3, e o MESMO
// programa escrito inline dava o valor certo.

const vals: any[] = [0, 0.5, 1, 1.5];

let inline = 0;
for (let i = 0; i < 4; i++) inline += vals[i] as number;

let viaConst = 0;
for (let i = 0; i < 4; i++) { const x = vals[i]; viaConst += x as number; }

let viaCastAtBind = 0;
for (let i = 0; i < 4; i++) { const x = vals[i] as number; viaCastAtBind += x; }

let chained = 0;
for (let i = 0; i < 4; i++) { const a = vals[i]; const b = a; chained += b as number; }

let squared = 0;
for (let i = 0; i < 4; i++) { const x = vals[i]; squared += (x as number) ** 2; }

let multiplied = 0;
for (let i = 0; i < 4; i++) { const x = vals[i]; multiplied += (x as number) * (x as number); }

// Fora de qualquer laço — o bug não dependia do loop.
const y = vals[1];
let outsideLoop = 0;
outsideLoop += y as number;

// Um acumulador que só vê inteiros NÃO deve ser promovido (o caminho int rápido
// continua valendo); este teste falha se a promoção virar incondicional.
let ints = 0;
for (let i = 0; i < 4; i++) ints += i;

describe("float promotion through bindings and casts", () => {
  test("inline heap read accumulates the fraction", () => {
    expect(inline).toBe(3);
  });
  test("through an intermediate const binding", () => {
    expect(viaConst).toBe(3);
  });
  test("with the cast at the binding instead of the use", () => {
    expect(viaCastAtBind).toBe(3);
  });
  test("through a chain of bindings", () => {
    expect(chained).toBe(3);
  });
  test("squared through a binding", () => {
    expect(squared).toBe(3.5);
  });
  test("multiplied through a binding", () => {
    expect(multiplied).toBe(3.5);
  });
  test("outside any loop", () => {
    expect(outsideLoop).toBe(0.5);
  });
  test("an integer-only accumulator still works", () => {
    expect(ints).toBe(6);
  });
});
