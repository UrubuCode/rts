// Simulação multi-usuário: N usuários, cada um gera eventos (ações) que mutam
// estado agregado. Mede throughput de processamento de eventos.
// Portável: roda igual em RTS, Node e Bun (sem APIs específicas de runtime).
//
// Modelo:
//  - USERS usuários, cada um com saldo (balance) e contador de ações.
//  - cada usuário gera EVENTS_PER_USER eventos; tipo do evento via PRNG.
//  - tipos: 0=deposit (+), 1=withdraw (-), 2=transfer (move p/ outro user),
//           3=action (incrementa contador global de ações processadas).
//  - estado mantido em arrays paralelos (balances[], actions[]) — o padrão
//    que o RTS otimiza (VEC_GET/SET/RMW).

const USERS = 5000;
const EVENTS_PER_USER = 2000;

// PRNG determinístico (LCG) — sem depender de wrap de 32-bit (que diverge
// entre runtimes p/ shift sobre variável). Usa só * + % com valores que
// cabem em f64 exato (< 2^53). Mesma sequência nos 3 runtimes.
let seed = 123456789;
function rnd(): number {
  // Park-Miller minimal standard: seed = (seed * 16807) % 2147483647
  seed = (seed * 16807) % 2147483647;
  return seed % 1000000;
}

const balances: number[] = [];
const actions: number[] = [];
for (let i = 0; i < USERS; i++) {
  balances[i] = 1000;
  actions[i] = 0;
}

let totalActions = 0;
let totalDeposits = 0;
let totalWithdraws = 0;
let totalTransfers = 0;

const t0 = Date.now();

for (let u = 0; u < USERS; u++) {
  for (let e = 0; e < EVENTS_PER_USER; e++) {
    const kind = rnd() % 4;
    if (kind === 0) {
      const amt = rnd() % 100;
      balances[u] = balances[u] + amt;
      totalDeposits = totalDeposits + 1;
    } else if (kind === 1) {
      const amt = rnd() % 100;
      if (balances[u] >= amt) {
        balances[u] = balances[u] - amt;
        totalWithdraws = totalWithdraws + 1;
      }
    } else if (kind === 2) {
      const target = rnd() % USERS;
      const amt = rnd() % 50;
      if (balances[u] >= amt) {
        balances[u] = balances[u] - amt;
        balances[target] = balances[target] + amt;
        totalTransfers = totalTransfers + 1;
      }
    } else {
      actions[u] = actions[u] + 1;
      totalActions = totalActions + 1;
    }
  }
}

const t1 = Date.now();

// Checksum: soma de todos os saldos (deve ser determinístico e idêntico nos 3).
let sumBalances = 0;
for (let i = 0; i < USERS; i++) {
  sumBalances = sumBalances + balances[i];
}

const totalEvents = USERS * EVENTS_PER_USER;
const ms = t1 - t0;

console.log("users=" + USERS + " events/user=" + EVENTS_PER_USER + " total_events=" + totalEvents);
console.log("deposits=" + totalDeposits + " withdraws=" + totalWithdraws + " transfers=" + totalTransfers + " actions=" + totalActions);
console.log("sum_balances=" + sumBalances);
console.log("time_ms=" + ms);
console.log("events_per_sec=" + Math.floor(totalEvents / (ms / 1000)));
