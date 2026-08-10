// Monte Carlo estimation of pi:
// throw N points into the unit square [0,1)x[0,1); count how many land
// inside the quarter circle x^2 + y^2 <= 1; pi ~ 4 * inside / N.
//
//   target/release/examples/run_fixture.exe bench/monte_carlo_pi.ts
//
// `math.seed`/`math.random_f64` do not exist on the new engine, and
// `Math.random` cannot be seeded — a benchmark whose input changes per run is
// not measuring the same work twice. First attempt was a 64-bit xorshift over
// `bigint`, but that hit three engine bugs on this run: `bigint << n` returns
// 0, `bigint & 0xffffffffffffffffn` returns NaN, and `Number(someBigint)`
// returns NaN unconditionally (see the report for repro). Ported instead to a
// plain Number-space Lehmer/Park-Miller LCG — the same family of generator
// `bench/rts_simple.ts` already exercises on this engine — seeded with the
// same constant (1) so every run draws the same sequence of points.

const N = 10_000_000;

let rngState: number = 1;
function nextRandomF64(): number {
  // Numerical Recipes LCG mod 2^32; acc*1664525 stays within 2^53 so this is
  // exact in f64 without ever needing bigint.
  rngState = (rngState * 1664525 + 1013904223) % 4294967296;
  if (rngState < 0) rngState = rngState + 4294967296;
  return rngState / 4294967296;
}

let inside: number = 0;
let i: number = 0;
while (i < N) {
  const x = nextRandomF64();
  const y = nextRandomF64();
  if (x * x + y * y <= 1.0) {
    inside = inside + 1;
  }
  i = i + 1;
}

// `inside` e `N` ja sao `number`, portanto a divisao ja e em f64.
//
// Aqui existia um `toFloat(x) = sqrt(x)*sqrt(x)`, que no motor antigo forcava
// um `i64` para float. Com os tipos portados o truque perdeu a razao e passou a
// custar: `sqrt(x)^2` nao devolve exatamente `x`, e o resultado saia
// 3.141037999999999 onde o Node responde 3.141038 — a comparacao com o .js
// media a diferenca do truque, nao a dos motores.
const pi: number = 4.0 * inside / N;
console.log(`N      = ${N}`);
console.log(`inside = ${inside}`);
console.log(`pi     = ${pi}`);
