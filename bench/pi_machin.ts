// Machin's formula: pi = 16 * atan(1/5) - 4 * atan(1/239)
// Precisao de maquina (double) em uma unica expressao.
//
//   target/release/examples/run_fixture.exe bench/pi_machin.ts
const pi: number = 16.0 * Math.atan(1.0 / 5.0) - 4.0 * Math.atan(1.0 / 239.0);

console.log(`pi (Machin)  = ${pi}`);
console.log(`pi (real)    = ${Math.PI}`);
console.log(`erro absoluto= ${Math.abs(pi - Math.PI)}`);
