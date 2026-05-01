// Bench: muitos eventos disparados de muitas threads — sync vs async.
//
// Cenario "real": servidor com N producers (cada um numa thread) que
// disparam eventos no mesmo emitter. Mede throughput total.
//
// Versao SYNC: cada producer dispara emit() na sua thread, listeners
// rodam ali mesmo. Naturalmente paralelo se ha multiplos producers.
//
// Versao ASYNC: cada producer dispara emit_async(), tasks vao pro pool
// tokio. Mais latencia por dispatch, mas pode aliviar producer.

import { events, atomic, time, io, thread } from "rts";

const counter = atomic.i64_new(0);
const totalEmits: i64 = 1000000;
const numProducers: i64 = 8;
const eventsPerProducer: i64 = totalEmits / numProducers;

function lightListener(_arg: i64): i64 {
  atomic.i64_fetch_add(counter, 1);
  return 0;
}

function reset(): void { atomic.i64_store(counter, 0); }

function waitFor(target: i64, timeoutMs: i64): void {
  const deadline = time.now_ms() + timeoutMs;
  while (atomic.i64_load(counter) < target) {
    if (time.now_ms() > deadline) {
      io.eprint("timeout: counter=" + atomic.i64_load(counter) + "\n");
      return;
    }
    time.sleep_ms(1);
  }
}

// Producer sync: dispara N events com emit() na sua propria thread.
function producerSync(em: i64): i64 {
  let i: i64 = 0;
  while (i < eventsPerProducer) {
    events.emit1(em, "tick", i);
    i = i + 1;
  }
  return 0;
}

// Producer async: usa emit1_async (vai pro pool tokio).
function producerAsync(em: i64): i64 {
  let i: i64 = 0;
  while (i < eventsPerProducer) {
    events.emit1_async(em, "tick", i);
    i = i + 1;
  }
  return 0;
}

io.print("=== " + numProducers + " producers x " + eventsPerProducer + " events = " + totalEmits + " total ===");
io.print("");

// Setup
const em = events.emitter_new();
events.on(em, "tick", lightListener);

// === SYNC: producers em std::thread::spawn. Listener roda na thread
// do producer. Paralelismo natural via N threads producers.
io.print("--- SYNC (producers em std::thread, listeners ali mesmo) ---");
reset();
const tSync = time.now_ns();
const handlesSync: i64[] = [];
let p: i64 = 0;
while (p < numProducers) {
  const h = thread.spawn(producerSync, em);
  handlesSync.push(h);
  p = p + 1;
}
let j: i64 = 0;
while (j < numProducers) {
  thread.join(handlesSync[j]);
  j = j + 1;
}
const syncMs = (time.now_ns() - tSync) / 1000000;
io.print("tempo total: " + syncMs + " ms");
io.print("counter:     " + atomic.i64_load(counter));
io.print("throughput:  " + (totalEmits * 1000 / syncMs) + " events/s");

// === ASYNC: producers usam emit_async (cada listener vai pro pool
// tokio). Producers terminam logo (so' enfileiram). Drain ate counter
// chegar no total.
io.print("");
io.print("--- ASYNC (emit_async dispatcha pro pool tokio) ---");
reset();
const tAsync = time.now_ns();
const handlesAsync: i64[] = [];
let p2: i64 = 0;
while (p2 < numProducers) {
  const h = thread.spawn(producerAsync, em);
  handlesAsync.push(h);
  p2 = p2 + 1;
}
let j2: i64 = 0;
while (j2 < numProducers) {
  thread.join(handlesAsync[j2]);
  j2 = j2 + 1;
}
const dispatchMs = (time.now_ns() - tAsync) / 1000000;
waitFor(totalEmits, 60000);
const drainMs = (time.now_ns() - tAsync) / 1000000;
io.print("dispatch:    " + dispatchMs + " ms");
io.print("drain total: " + drainMs + " ms");
io.print("counter:     " + atomic.i64_load(counter));
io.print("throughput:  " + (totalEmits * 1000 / drainMs) + " events/s");

io.print("");
io.print("done.");

events.emitter_free(em);
