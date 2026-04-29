import { io } from "rts";

// ── Sync EventEmitter ─────────────────────────────────────────────────────────

const ee = new EventEmitter();

let called = 0;
function onData(arg: number): number {
  called = called + arg;
  return 0;
}

ee.on("data", onData);
io.print(ee.listenerCount("data") === 1 ? "count_ok" : "count_fail");

ee.emit("data", 42);
io.print(called === 42 ? "emit_ok" : "emit_fail");

ee.emit("data", 8);
io.print(called === 50 ? "emit2_ok" : "emit2_fail");

// .once — fires once then auto-removes
let onceFired = 0;
function onOnce(arg: number): number {
  onceFired = onceFired + 1;
  return 0;
}

ee.once("ping", onOnce);
ee.emit("ping", 0);
ee.emit("ping", 0);
io.print(onceFired === 1 ? "once_ok" : "once_fail");

// .off — removes listener
ee.off("data", onData);
io.print(ee.listenerCount("data") === 0 ? "off_ok" : "off_fail");

// .emit returns false when no listeners
const had = ee.emit("data", 1);
io.print(!had ? "no_listeners_ok" : "no_listeners_fail");

// .removeAllListeners
ee.on("x", onData);
ee.on("x", onOnce);
ee.removeAllListeners("x");
io.print(ee.listenerCount("x") === 0 ? "remove_all_ok" : "remove_all_fail");

// ── Async EventEmitter ────────────────────────────────────────────────────────

// Async mode — just verify it doesn't crash and listeners are registered
const ae = new EventEmitter(true);
ae.on("data", onData);
io.print(ae.listenerCount("data") === 1 ? "async_count_ok" : "async_count_fail");
// emit in async mode (rayon) — wait not possible in sync test, just check no panic
ae.emit("data", 1);
io.print("async_emit_no_crash");
