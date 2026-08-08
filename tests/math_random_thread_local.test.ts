import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #281: PRNG state migrado de `static mut` global para thread_local.
//
// ESTE TESTE PASSOU A CHECAR MENOS, e o motivo é da superfície, não do motor:
// `math.seed` + `math.random_f64` viraram `Math.random`, que a spec proíbe
// EXPLICITAMENTE de ser semeável. A asserção "mesma seed → mesma sequência" —
// que era como o #281 provava que o estado é por-thread e não global — não tem
// equivalente honesto em `Math.random` e por isso saiu. Recuperá-la exige um
// PRNG escrito no próprio teste, não esta API.
//
// O que continua verificado é o que `Math.random` de fato promete: valor em
// [0, 1) e uma sequência que AVANÇA em vez de ficar presa num valor.

const r1 = Math.random();
const r2 = Math.random();
const r3 = Math.random();
const r4 = Math.random();

// Sequencia avanca: quatro sorteios seguidos nao repetem.
const advance = (r1 !== r2 && r2 !== r3 && r3 !== r4) ? "advance" : "stuck";
print(advance);

// Range esta em [0, 1).
const inrange = (r1 >= 0.0 && r1 < 1.0) && (r2 >= 0.0 && r2 < 1.0)
  && (r3 >= 0.0 && r3 < 1.0) && (r4 >= 0.0 && r4 < 1.0);
const range = inrange ? "range" : "out";
print(range);

describe("fixture:math_random_thread_local", () => {
  // sem "seed determinism": ver o comentário no topo — `Math.random` não é
  // semeável, então essa parte da afirmação não existe mais nesta superfície.
  test("Math.random avanca e fica em [0, 1)", () => {
    expect(__rtsCapturedOutput).toBe("advance\nrange\n");
  });
});
