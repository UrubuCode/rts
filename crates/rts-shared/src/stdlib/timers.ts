// Global timer functions — rts-shared stdlib prelude (NOT primordial; no native
// syntax). Pure TS over the private `engine.*` timer bridges: the fn VALUE rides
// as a PolyValue word into the runtime's ordered macrotask/microtask queues
// (pumped by `promise.wait` / `time.sleep_ms` / the post-main drain), and the
// pump invokes it through the bound-env-aware bridge — a CAPTURING arrow works.

function setTimeout(cb: any, ms?: any): any {
  return engine.set_timeout(cb, ms ?? 0);
}

function clearTimeout(id: any): any {
  return engine.clear_timer(id);
}

function setInterval(cb: any, ms?: any): any {
  return engine.set_interval(cb, ms ?? 0);
}

function clearInterval(id: any): any {
  return engine.clear_timer(id);
}

function queueMicrotask(cb: any): any {
  return engine.queue_microtask(cb);
}

function setImmediate(cb: any): any {
  return engine.set_immediate(cb);
}

function clearImmediate(id: any): any {
  return engine.clear_timer(id);
}
