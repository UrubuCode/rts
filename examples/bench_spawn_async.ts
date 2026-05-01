// Bench de spawn rate puro: thread.spawn vs thread.spawn_async vs spawn_detached
//
// Mede apenas o custo de submeter N tarefas curtas (sem aguardar). Em
// servidor HTTP isso e' o gargalo critico: cada request precisa
// dispatchar um worker rapidamente, sem bloquear o accept loop.

import { thread, time, io } from "rts";

const N = 10000;

function work(_arg: i64): i64 {
  return 0;
}

// Warmup
for (let i = 0; i < 100; i = i + 1) {
  thread.spawn_async(work, 0);
}
time.sleep_ms(200);

io.print("=== Spawn rate (10000 spawns) ===");

// 1. std::thread::spawn por chamada
const a = time.now_ns();
for (let i = 0; i < N; i = i + 1) {
  const h = thread.spawn(work, 0);
  thread.detach(h);
}
const aMs = (time.now_ns() - a) / 1000000;
io.print("spawn std::thread:   " + aMs + " ms (" + ((N * 1.0) / aMs) + "k spawns/s)");

// 2. spawn_async (tokio spawn_blocking)
const b = time.now_ns();
for (let i = 0; i < N; i = i + 1) {
  thread.spawn_async(work, 0);
}
const bMs = (time.now_ns() - b) / 1000000;
io.print("spawn_async tokio:   " + bMs + " ms (" + ((N * 1.0) / bMs) + "k spawns/s)");

// 3. spawn_detached (nosso pool)
const c = time.now_ns();
for (let i = 0; i < N; i = i + 1) {
  thread.spawn_detached(work, 0);
}
const cMs = (time.now_ns() - c) / 1000000;
io.print("spawn_detached pool: " + cMs + " ms (" + ((N * 1.0) / cMs) + "k spawns/s)");

// Espera as queues drenarem antes de sair (senao corta tasks pendentes)
time.sleep_ms(2000);
io.print("done.");
