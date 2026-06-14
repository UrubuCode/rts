// Simulação SEM arrays: lógica de negócio pura com aritmética intensa,
// funções, branches e acumuladores escalares. Mede o codegen de
// call/branch/arith — não acesso a coleção.
// Portável: roda igual em RTS, Node, Bun.

// PRNG determinístico (Park-Miller LCG).
let seed = 123456789;
function rnd(): number { seed = (seed * 16807) % 2147483647; return seed % 1000000; }

// "Regras de negócio" — funções pequenas (candidatas a inline).
function tax(income: number): number {
  if (income < 1000) return 0;
  if (income < 5000) return (income * 10) / 100;
  if (income < 20000) return (income * 20) / 100;
  return (income * 35) / 100;
}

function bonus(perf: number, base: number): number {
  if (perf > 90) return (base * 20) / 100;
  if (perf > 70) return (base * 10) / 100;
  return 0;
}

function score(a: number, b: number, c: number): number {
  return (a * 3 + b * 2 + c) % 1000;
}

const ITER = 30000000;

let totalTax = 0;
let totalBonus = 0;
let totalScore = 0;
let highEarners = 0;

const t0 = Date.now();

for (let i = 0; i < ITER; i++) {
  const income = rnd() % 50000;
  const perf = rnd() % 100;
  const base = rnd() % 10000;

  const t = tax(income);
  const b = bonus(perf, base);
  const s = score(income, perf, base);

  totalTax = (totalTax + t) % 1000000007;
  totalBonus = (totalBonus + b) % 1000000007;
  totalScore = (totalScore + s) % 1000000007;
  if (income > 20000) highEarners = highEarners + 1;
}

const t1 = Date.now();

console.log("iters=" + ITER + " high_earners=" + highEarners);
console.log("total_tax=" + totalTax + " total_bonus=" + totalBonus + " total_score=" + totalScore);
console.log("time_ms=" + (t1 - t0) + " iters_per_sec=" + Math.floor(ITER / ((t1 - t0) / 1000)));
