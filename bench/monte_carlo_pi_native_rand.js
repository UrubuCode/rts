// Baseline JS usando Math.random() nativo (PRNG do engine).
// Algoritmo diferente de RTS mas mede só a velocidade do loop + RNG engine.

const N = 10_000_000;
let inside = 0;
for (let i = 0; i < N; i++) {
  const x = Math.random();
  const y = Math.random();
  if (x * x + y * y <= 1.0) inside++;
}
console.log(`N      = ${N}`);
console.log(`inside = ${inside}`);
console.log(`pi     = ${4 * inside / N}`);
