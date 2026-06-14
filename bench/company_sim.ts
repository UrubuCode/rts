// Simulação de empresa: N funcionários, M departamentos, processando eventos
// (vendas, despesas, transferências entre deptos, contratações). Estado
// agregado em arrays paralelos pré-dimensionados (padrão otimizável pelo RTS).
// Portável: roda igual em RTS, Node e Bun.

const EMPLOYEES = 10000;
const DEPARTMENTS = 50;
const EVENTS = 20000000;

// Estado por funcionário (arrays paralelos, pré-dimensionados).
const salary: number[] = new Array(EMPLOYEES);
const performance: number[] = new Array(EMPLOYEES);
const deptOf: number[] = new Array(EMPLOYEES);
for (let i = 0; i < EMPLOYEES; i++) {
  salary[i] = 50000;
  performance[i] = 100;
  deptOf[i] = i % DEPARTMENTS;
}

// Orçamento por departamento.
const deptBudget: number[] = new Array(DEPARTMENTS);
for (let d = 0; d < DEPARTMENTS; d++) deptBudget[d] = 1000000;

// PRNG determinístico (Park-Miller LCG).
let seed = 123456789;
function rnd(): number { seed = (seed * 16807) % 2147483647; return seed % 1000000; }

let totalSales = 0;
let totalRaises = 0;
let totalTransfers = 0;

const t0 = Date.now();

for (let ev = 0; ev < EVENTS; ev++) {
  const emp = rnd() % EMPLOYEES;
  const kind = rnd() % 4;

  if (kind === 0) {
    // venda: adiciona ao orçamento do depto do funcionário + bump performance
    const amount = rnd() % 5000;
    const dept = deptOf[emp];
    deptBudget[dept] = deptBudget[dept] + amount;
    performance[emp] = performance[emp] + 1;
    totalSales = totalSales + 1;
  } else if (kind === 1) {
    // aumento: se performance alta e orçamento do depto cobre
    const dept = deptOf[emp];
    const raise = rnd() % 2000;
    if (performance[emp] > 105 && deptBudget[dept] >= raise) {
      salary[emp] = salary[emp] + raise;
      deptBudget[dept] = deptBudget[dept] - raise;
      totalRaises = totalRaises + 1;
    }
  } else if (kind === 2) {
    // transferência entre departamentos
    const toDept = rnd() % DEPARTMENTS;
    const fromDept = deptOf[emp];
    const amount = rnd() % 1000;
    if (deptBudget[fromDept] >= amount) {
      deptBudget[fromDept] = deptBudget[fromDept] - amount;
      deptBudget[toDept] = deptBudget[toDept] + amount;
      deptOf[emp] = toDept;
      totalTransfers = totalTransfers + 1;
    }
  } else {
    // penalidade de performance
    if (performance[emp] > 50) performance[emp] = performance[emp] - 1;
  }
}

const t1 = Date.now();

// Checksums determinísticos.
let totalSalary = 0;
let totalBudget = 0;
let totalPerf = 0;
for (let i = 0; i < EMPLOYEES; i++) { totalSalary = totalSalary + salary[i]; totalPerf = totalPerf + performance[i]; }
for (let d = 0; d < DEPARTMENTS; d++) totalBudget = totalBudget + deptBudget[d];

console.log("events=" + EVENTS + " sales=" + totalSales + " raises=" + totalRaises + " transfers=" + totalTransfers);
console.log("total_salary=" + totalSalary + " total_budget=" + totalBudget + " total_perf=" + totalPerf);
console.log("time_ms=" + (t1 - t0) + " events_per_sec=" + Math.floor(EVENTS / ((t1 - t0) / 1000)));
