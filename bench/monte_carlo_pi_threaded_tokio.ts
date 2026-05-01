// Monte Carlo Pi multi-thread via spawn_async_join (tokio runtime
// compartilhado, issue #399). Equivalente do bench
// monte_carlo_pi_threaded.ts mas usando o sistema de thread novo.

import { io, math, thread } from "rts";

const N: i64 = 10_000_000;
const W: i64 = 8;
const PER_WORKER: i64 = N / W;

function toFloat(x: i64): f64 {
  return math.sqrt(x) * math.sqrt(x);
}

function worker(seed: i64): i64 {
  math.seed(seed);
  let local: i64 = 0;
  let i: i64 = 0;
  while (i < PER_WORKER) {
    const x = math.random_f64();
    const y = math.random_f64();
    if (x * x + y * y <= 1.0) {
      local = local + 1;
    }
    i = i + 1;
  }
  return local;
}

const handles: i64[] = [];
let w: i64 = 0;
while (w < W) {
  // spawn_async_join: roda em tokio::spawn_blocking, retorna id pra
  // join_async pegar o valor.
  const id = thread.spawn_async_join(worker, w + 1);
  handles.push(id);
  w = w + 1;
}

let inside: i64 = 0;
let j: i64 = 0;
while (j < W) {
  inside = inside + (thread.join_async(handles[j]) as i64);
  j = j + 1;
}

const pi: f64 = 4.0 * toFloat(inside) / toFloat(N);
io.print(`N       = ${N}`);
io.print(`workers = ${W}`);
io.print(`inside  = ${inside}`);
io.print(`pi      = ${pi}`);
