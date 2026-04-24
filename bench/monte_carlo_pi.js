// Equivalente em JS para comparar velocidade — usa o xorshift64
// para que RTS e JS rodem o mesmo algoritmo com a mesma sequência.

let state = 1n;
function seed(s) { state = BigInt(s) || 1n; }
function rand() {
  let x = state;
  x ^= (x << 13n) & 0xFFFFFFFFFFFFFFFFn;
  x ^= x >> 7n;
  x ^= (x << 17n) & 0xFFFFFFFFFFFFFFFFn;
  state = x & 0xFFFFFFFFFFFFFFFFn;
  return Number(state >> 11n) / Number(1n << 53n);
}

const N = 10_000_000;
seed(1);
let inside = 0;
for (let i = 0; i < N; i++) {
  const x = rand();
  const y = rand();
  if (x * x + y * y <= 1.0) inside++;
}
const pi = 4 * inside / N;
console.log(`N      = ${N}`);
console.log(`inside = ${inside}`);
console.log(`pi     = ${pi}`);
