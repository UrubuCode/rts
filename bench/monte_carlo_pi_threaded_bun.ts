// Bun multi-thread Monte Carlo Pi via Worker threads.
// Equivalente do bench/monte_carlo_pi_threaded.ts (RTS).
//
// 8 workers cada um faz N/8 iteracoes com seu RNG (Math.random,
// que e' thread-local em cada Worker).
//
// Uso: bun run bench/monte_carlo_pi_threaded_bun.ts

const N = 10_000_000;
const W = 8;
const PER_WORKER = Math.floor(N / W);

// Worker code inline (Blob URL).
const workerSrc = `
self.onmessage = (e) => {
  const per = e.data;
  let inside = 0;
  for (let i = 0; i < per; i++) {
    const x = Math.random();
    const y = Math.random();
    if (x*x + y*y <= 1.0) inside++;
  }
  postMessage(inside);
};
`;

const blob = new Blob([workerSrc], { type: "application/javascript" });
const url = URL.createObjectURL(blob);

const promises = Array.from({ length: W }, () => {
  return new Promise<number>((resolve) => {
    const w = new Worker(url);
    w.onmessage = (e) => {
      resolve(e.data as number);
      w.terminate();
    };
    w.postMessage(PER_WORKER);
  });
});

const start = performance.now();
const results = await Promise.all(promises);
const dt = performance.now() - start;

const inside = results.reduce((a, b) => a + b, 0);
const pi = (4 * inside) / N;

console.log(`N       = ${N}`);
console.log(`workers = ${W}`);
console.log(`inside  = ${inside}`);
console.log(`pi      = ${pi}`);
console.log(`time    = ${dt.toFixed(1)} ms`);
