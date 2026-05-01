// Bench: events.emit (sync sequencial Node-style) vs events.emit1_async
// (paralelo via tokio spawn_blocking).

import { events, atomic, time, io } from "rts";

const counter = atomic.i64_new(0);

function lightListener(_arg: i64): i64 {
  atomic.i64_fetch_add(counter, 1);
  return 0;
}

function mediumListener(_arg: i64): i64 {
  let sum: i64 = 0;
  let i: i64 = 0;
  while (i < 1000) {
    sum = sum + i * 7;
    i = i + 1;
  }
  atomic.i64_fetch_add(counter, sum & 1);
  return 0;
}

function nowNs(): i64 { return time.now_ns(); }
function elapsedMs(start: i64): i64 { return (time.now_ns() - start) / 1000000; }

function reset(): void { atomic.i64_store(counter, 0); }

function waitFor(target: i64, timeoutMs: i64): void {
  const deadline = time.now_ms() + timeoutMs;
  while (atomic.i64_load(counter) < target) {
    if (time.now_ms() > deadline) {
      io.eprint("timeout: counter=" + atomic.i64_load(counter) + " target=" + target + "\n");
      return;
    }
    time.sleep_ms(1);
  }
}

function spamSync(em: i64, n: i64, name: string): i64 {
  let i: i64 = 0;
  while (i < n) {
    events.emit1(em, name, i);
    i = i + 1;
  }
  return 0;
}

function spamAsync(em: i64, n: i64, name: string): i64 {
  let i: i64 = 0;
  while (i < n) {
    events.emit1_async(em, name, i);
    i = i + 1;
  }
  return 0;
}

// === Cenario 1: 1 listener leve, 100k events ===
io.print("=== Cenario 1: 1 listener leve (atomic add), 100k events ===");
const em1 = events.emitter_new();
events.on(em1, "tick", lightListener);

reset();
let t1 = nowNs();
spamSync(em1, 100000, "tick");
io.print("emit sync   100k:                       " + elapsedMs(t1) + " ms (counter=" + atomic.i64_load(counter) + ")");

reset();
t1 = nowNs();
spamAsync(em1, 100000, "tick");
const dispatch1 = elapsedMs(t1);
waitFor(100000, 30000);
const drain1 = elapsedMs(t1);
io.print("emit async  100k dispatch:              " + dispatch1 + " ms");
io.print("emit async  100k drain:                 " + drain1 + " ms (counter=" + atomic.i64_load(counter) + ")");

events.emitter_free(em1);

// === Cenario 2: 1 listener medio, 10k events ===
io.print("");
io.print("=== Cenario 2: 1 listener medio (1k loop), 10k events ===");
const em2 = events.emitter_new();
events.on(em2, "work", mediumListener);

reset();
let t2 = nowNs();
spamSync(em2, 10000, "work");
io.print("emit sync   10k:                        " + elapsedMs(t2) + " ms");

reset();
t2 = nowNs();
spamAsync(em2, 10000, "work");
const dispatch2 = elapsedMs(t2);
waitFor(10000, 30000);
const drain2 = elapsedMs(t2);
io.print("emit async  10k dispatch:               " + dispatch2 + " ms");
io.print("emit async  10k drain:                  " + drain2 + " ms");

events.emitter_free(em2);

// === Cenario 3: 50 listeners leves, 1k events ===
io.print("");
io.print("=== Cenario 3: 50 listeners leves, 1k events ===");
const em3 = events.emitter_new();
let i3: i64 = 0;
while (i3 < 50) { events.on(em3, "fan", lightListener); i3 = i3 + 1; }

reset();
let t3 = nowNs();
spamSync(em3, 1000, "fan");
io.print("emit sync   1k * 50 listeners:          " + elapsedMs(t3) + " ms (counter=" + atomic.i64_load(counter) + ")");

reset();
t3 = nowNs();
spamAsync(em3, 1000, "fan");
const dispatch3 = elapsedMs(t3);
waitFor(50000, 30000);
const drain3 = elapsedMs(t3);
io.print("emit async  1k * 50 dispatch:           " + dispatch3 + " ms");
io.print("emit async  1k * 50 drain:              " + drain3 + " ms (counter=" + atomic.i64_load(counter) + ")");

events.emitter_free(em3);

// === Cenario 4: 50 listeners medios, 100 events ===
io.print("");
io.print("=== Cenario 4: 50 listeners medios, 100 events ===");
const em4 = events.emitter_new();
let i4: i64 = 0;
while (i4 < 50) { events.on(em4, "heavy", mediumListener); i4 = i4 + 1; }

reset();
let t4 = nowNs();
spamSync(em4, 100, "heavy");
io.print("emit sync   100 * 50 listeners medios:  " + elapsedMs(t4) + " ms");

reset();
t4 = nowNs();
spamAsync(em4, 100, "heavy");
const dispatch4 = elapsedMs(t4);
waitFor(5000, 30000);
const drain4 = elapsedMs(t4);
io.print("emit async  100 * 50 dispatch:          " + dispatch4 + " ms");
io.print("emit async  100 * 50 drain:             " + drain4 + " ms");

events.emitter_free(em4);

io.print("");
io.print("done.");
