// Monte Carlo Pi multi-thread.
//
// W workers rodam em paralelo, cada um com seu RNG (math.random_f64
// usa estado thread-local). Cada worker retorna seu count via
// thread.join (u64 retorno). Sem atomic — mais simples.

import { io, math, thread } from "rts";

// Hack do bench single-thread: forca i64 -> f64 via sqrt(x)*sqrt(x)
// (== x mas o intermediario e' f64).
function toFloat(x: i64): f64 {
  return math.sqrt(x) * math.sqrt(x);
}

const N: i64 = 10_000_000;
const W: i64 = 8;
const PER_WORKER: i64 = N / W;

function worker(seed: i64): i64 {
  // math.seed e' thread-local — cada worker tem seu RNG isolado.
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
  const h = thread.spawn(worker, w + 1);
  handles.push(h);
  w = w + 1;
}

let inside: i64 = 0;
let j: i64 = 0;
while (j < W) {
  inside = inside + (thread.join(handles[j]) as i64);
  j = j + 1;
}

const pi: f64 = 4.0 * toFloat(inside) / toFloat(N);
io.print(`N       = ${N}`);
io.print(`workers = ${W}`);
io.print(`inside  = ${inside}`);
io.print(`pi      = ${pi}`);
