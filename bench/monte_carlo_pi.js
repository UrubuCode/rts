// Equivalente em JS para comparar velocidade — corre O MESMO algoritmo e a
// MESMA sequencia que bench/monte_carlo_pi.ts, que e a unica razao deste
// ficheiro existir. Se os dois divergirem, a comparacao mede duas cargas
// diferentes e nao diz nada sobre os motores.
//
//   node bench/monte_carlo_pi.js
//   bun  bench/monte_carlo_pi.js
//
// Era um xorshift64 sobre `bigint`. Passou a LCG em espaco de Number quando o
// .ts teve de sair do bigint: no motor novo `bigint << n` da 0, `bigint &
// 0xffffffffffffffffn` da NaN e `Number(umBigint)` da NaN, portanto o .ts nao
// tinha como manter o xorshift. Mudar so um dos dois lados e o que quebraria o
// par — por isso este mudou junto.

const N = 10_000_000;

let rngState = 1;
function nextRandomF64() {
  // Numerical Recipes LCG mod 2^32; acc*1664525 cabe em 2^53, logo e exato em
  // f64 sem precisar de bigint.
  rngState = (rngState * 1664525 + 1013904223) % 4294967296;
  if (rngState < 0) rngState = rngState + 4294967296;
  return rngState / 4294967296;
}

let inside = 0;
let i = 0;
while (i < N) {
  const x = nextRandomF64();
  const y = nextRandomF64();
  if (x * x + y * y <= 1.0) inside++;
  i++;
}
const pi = 4 * inside / N;
console.log(`N      = ${N}`);
console.log(`inside = ${inside}`);
console.log(`pi     = ${pi}`);
