// Cross-runtime: UMA coisa — função recursiva que fecha sobre um acumulador do
// escopo de FORA: todas as frames da recursão compartilham o MESMO binding, então
// as mutações se somam através da pilha (não há uma cópia por frame). Variações:
// contador de chamadas, coleta de trilha, profundidade máxima, memo compartilhado,
// mutual recursion sobre o mesmo acc, e o contraste com acumulador por-frame.

// 1) acumulador compartilhado: soma através das frames
function sumTo(n: number): number {
  let total = 0;
  function go(k: number): void {
    if (k <= 0) return;
    total += k;
    go(k - 1);
  }
  go(n);
  return total;
}
console.log("shared_total=" + sumTo(5));

// 2) contador de chamadas via closure externa
function countCalls(n: number): string {
  let calls = 0;
  function fib(k: number): number {
    calls += 1;
    return k < 2 ? k : fib(k - 1) + fib(k - 2);
  }
  const v = fib(n);
  return v + "/" + calls;
}
console.log("fib_calls=" + countCalls(6));

// 3) trilha de visitação (ordem de entrada/saída) num array capturado
function trail(n: number): string {
  const out: string[] = [];
  function walk(k: number): void {
    out.push("in" + k);
    if (k > 0) walk(k - 1);
    out.push("out" + k);
  }
  walk(n);
  return out.join(",");
}
console.log("trail=" + trail(2));

// 4) profundidade máxima observada via binding compartilhado
function maxDepth(n: number): number {
  let depth = 0;
  let best = 0;
  function go(k: number): void {
    depth += 1;
    if (depth > best) best = depth;
    if (k > 0) {
      go(k - 1);
      go(k - 1);
    }
    depth -= 1;
  }
  go(n);
  return best;
}
console.log("max_depth=" + maxDepth(3));

// 5) memo compartilhado entre frames
function memoFib(n: number): string {
  const memo: Record<number, number> = {};
  let hits = 0;
  function f(k: number): number {
    if (k in memo) {
      hits += 1;
      return memo[k];
    }
    const r = k < 2 ? k : f(k - 1) + f(k - 2);
    memo[k] = r;
    return r;
  }
  const v = f(n);
  return v + "/hits=" + hits;
}
console.log("memo=" + memoFib(10));

// 6) mutual recursion sobre o MESMO acumulador
function mutual(n: number): string {
  const log: string[] = [];
  function even(k: number): boolean {
    log.push("e" + k);
    return k === 0 ? true : odd(k - 1);
  }
  function odd(k: number): boolean {
    log.push("o" + k);
    return k === 0 ? false : even(k - 1);
  }
  const r = even(n);
  return r + ":" + log.join("");
}
console.log("mutual=" + mutual(4));

// 7) contraste: acumulador declarado DENTRO da fn recursiva é por-frame
function perFrame(n: number): string {
  function go(k: number): number {
    let local = k;
    if (k > 0) {
      go(k - 1);
    }
    return local;
  }
  return "" + go(3);
}
console.log("per_frame=" + perFrame(3));

// 8) acumulador externo sobrevive entre chamadas top-level distintas
let globalCalls = 0;
function tick(k: number): number {
  globalCalls += 1;
  return k > 0 ? tick(k - 1) : globalCalls;
}
console.log("first=" + tick(2));
console.log("second=" + tick(2));
console.log("global_total=" + globalCalls);
