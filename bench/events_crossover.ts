// Bench: ponto de cruzamento sync vs async em events.
// Passa work-iters via `arg` do listener pra evitar bug de global
// mutavel nao-thread-shared (#407).

import { events, atomic, time, io } from "rts";

const counter = atomic.i64_new(0);

// Listener: recebe iters via arg. Loop computa sum, atomic add ao counter.
function calibratedListener(iters: i64): i64 {
  let sum: i64 = 0;
  let i: i64 = 0;
  while (i < iters) {
    sum = sum + i * 7;
    i = i + 1;
  }
  // Sempre incrementa por 1, garantindo que counter rastreia chamadas
  // (independente do trabalho).
  atomic.i64_fetch_add(counter, 1);
  return sum;
}

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

function calibrate(iters: i64, samples: i64): i64 {
  const start = time.now_ns();
  let i: i64 = 0;
  while (i < samples) {
    calibratedListener(iters);
    i = i + 1;
  }
  return (time.now_ns() - start) / samples;
}

function runScenario(numListeners: i64, numEvents: i64, iters: i64): void {
  const perCallNs = calibrate(iters, 1000);
  io.print("--- " + numListeners + " listeners x " + numEvents + " events, listener~" + perCallNs + " ns ---");

  const em = events.emitter_new();
  let i: i64 = 0;
  while (i < numListeners) {
    events.on(em, "tick", calibratedListener);
    i = i + 1;
  }

  // SYNC
  reset();
  let t = time.now_ns();
  let j: i64 = 0;
  while (j < numEvents) {
    events.emit1(em, "tick", iters);
    j = j + 1;
  }
  const syncMs = (time.now_ns() - t) / 1000000;
  const totalCalls = numListeners * numEvents;

  // ASYNC
  reset();
  t = time.now_ns();
  let k: i64 = 0;
  while (k < numEvents) {
    events.emit1_async(em, "tick", iters);
    k = k + 1;
  }
  const dispatchMs = (time.now_ns() - t) / 1000000;
  waitFor(totalCalls, 60000);
  const drainMs = (time.now_ns() - t) / 1000000;

  io.print("  sync:  " + syncMs + " ms  drain (" + (totalCalls * 1000 / syncMs) + " calls/s)");
  io.print("  async: " + drainMs + " ms drain (" + (totalCalls * 1000 / drainMs) + " calls/s) [dispatch=" + dispatchMs + "ms]");
  if (drainMs > 0 && drainMs < syncMs) {
    io.print("  -> ASYNC GANHA " + (syncMs / drainMs) + "x");
  } else if (syncMs > 0) {
    io.print("  -> sync ganha " + (drainMs / syncMs) + "x");
  }

  events.emitter_free(em);
}

io.print("=== Variando NUM LISTENERS, listener leve (iters=0) ===");
runScenario(1, 1000, 0);
runScenario(8, 1000, 0);
runScenario(64, 1000, 0);
runScenario(256, 1000, 0);

io.print("");
io.print("=== Variando CUSTO listener (8 listeners, 1k events) ===");
runScenario(8, 1000, 100);
runScenario(8, 1000, 1000);
runScenario(8, 1000, 5000);
runScenario(8, 1000, 20000);
runScenario(8, 100, 100000);

io.print("");
io.print("=== Combinacao: muitos listeners + medianos/pesados ===");
runScenario(64, 100, 5000);
runScenario(256, 100, 5000);
runScenario(64, 100, 20000);

io.print("");
io.print("done.");
