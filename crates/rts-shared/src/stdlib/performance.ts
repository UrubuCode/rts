// `performance` — the global high-resolution timer (a rts-shared backend utility,
// NOT a primordial; no native syntax). Pure TS over the PRIVATE engine clock
// bridges (`engine.now_ms` monotonic, `engine.unix_ms` wall-clock epoch) — the
// engine names nothing about `performance`; `performance.now()` is an ordinary
// member call on this ambient singleton, exactly like `console`.

class Performance {
  // Monotonic milliseconds since an arbitrary origin (the process clock). JS
  // returns sub-ms float resolution; the engine clock is ms-granular here, which
  // satisfies the monotonic contract (`b >= a` for later calls).
  now(): number {
    return engine.now_ms();
  }

  // The wall-clock epoch (ms) at which `now()` reads 0 — i.e. the time origin.
  // `unix_ms()` is the current epoch, `now_ms()` the monotonic offset, so their
  // difference is the (≈constant) epoch of the monotonic origin.
  get timeOrigin(): number {
    return engine.unix_ms() - engine.now_ms();
  }
}

const performance: Performance = new Performance();
