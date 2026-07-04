// Web event-model classes — Event, EventTarget, AbortSignal, AbortController,
// MessageChannel/MessagePort — rts-shared stdlib prelude (NOT primordial; no
// native syntax). Pure `.ts` state machines: EventTarget keeps parallel
// listener arrays and dispatches synchronously (spec order); AbortController
// flips its AbortSignal and fires "abort"; MessagePort delivers via
// queueMicrotask. `AbortSignal.timeout` arms a real setTimeout — it flips the
// signal when the engine's timer pump reaches it (the cross-thread timer
// ordering under `await` is #207; until then a mid-await timeout may fire only
// at the post-main drain).

class Event {
  type: string;
  defaultPrevented: boolean = false;
  cancelable: boolean = false;
  bubbles: boolean = false;
  target: any = null;
  currentTarget: any = null;
  constructor(type: any, opts: any = undefined) {
    this.type = "" + type;
    if (opts !== undefined && opts !== null) {
      const c = opts.cancelable;
      if (c !== undefined) { this.cancelable = !!c; }
      const b = opts.bubbles;
      if (b !== undefined) { this.bubbles = !!b; }
    }
  }
  preventDefault(): void {
    if (this.cancelable) { this.defaultPrevented = true; }
  }
  stopPropagation(): void {}
  stopImmediatePropagation(): void {}
}

class EventTarget {
  #types: string[] = [];
  #cbs: any[] = [];
  #onces: boolean[] = [];
  // Duplicate (type, callback) pairs are ignored (spec).
  addEventListener(type: any, cb: any, opts: any = undefined): void {
    const t = "" + type;
    for (let i = 0; i < this.#types.length; i++) {
      if (this.#types[i] === t && this.#cbs[i] === cb) return;
    }
    let once = false;
    if (opts !== undefined && opts !== null) {
      const o = opts.once;
      if (o !== undefined) { once = !!o; }
    }
    this.#types.push(t);
    this.#cbs.push(cb);
    this.#onces.push(once);
  }
  removeEventListener(type: any, cb: any): void {
    const t = "" + type;
    const kt: string[] = [];
    const kc: any[] = [];
    const ko: boolean[] = [];
    for (let i = 0; i < this.#types.length; i++) {
      if (this.#types[i] === t && this.#cbs[i] === cb) continue;
      kt.push(this.#types[i]); kc.push(this.#cbs[i]); ko.push(this.#onces[i]);
    }
    this.#types = kt; this.#cbs = kc; this.#onces = ko;
  }
  // Synchronous dispatch in registration order; `once` listeners are removed
  // BEFORE their callback runs (spec). Returns `!defaultPrevented`.
  dispatchEvent(ev: any): boolean {
    ev.target = this;
    ev.currentTarget = this;
    const t = "" + ev.type;
    const run: any[] = [];
    const runOnce: boolean[] = [];
    for (let i = 0; i < this.#types.length; i++) {
      if (this.#types[i] === t) { run.push(this.#cbs[i]); runOnce.push(this.#onces[i]); }
    }
    for (let i = 0; i < run.length; i++) {
      if (runOnce[i]) { this.removeEventListener(t, run[i]); }
      const cb = run[i];
      cb(ev);
    }
    return !ev.defaultPrevented;
  }
}

class AbortSignal extends EventTarget {
  aborted: boolean = false;
  reason: any = undefined;
  onabort: any = null;
  // Internal transition (double-underscore: called by AbortController /
  // statics; a `#name` would be lexically confined to THIS class body).
  __doAbort(reason: any): void {
    if (this.aborted) return;
    this.aborted = true;
    this.reason = reason;
    const ev = new Event("abort");
    this.dispatchEvent(ev);
    const h = this.onabort;
    if (h !== null && h !== undefined) { h(ev); }
  }
  throwIfAborted(): void {
    if (this.aborted) { throw this.reason; }
  }
  static abort(reason: any = undefined): AbortSignal {
    const s = new AbortSignal();
    if (reason === undefined) {
      s.__doAbort(new DOMException("This operation was aborted", "AbortError"));
    } else {
      s.__doAbort(reason);
    }
    return s;
  }
  static timeout(ms: any): AbortSignal {
    const s = new AbortSignal();
    setTimeout(() => {
      // Hoisted: an inline `new DOMException(...)` ARG makes the captured-`s`
      // dynamic dispatch decline (effectful-arg rule) — an Ident arg is fine.
      const err = new DOMException("The operation timed out.", "TimeoutError");
      s.__doAbort(err);
    }, ms);
    return s;
  }
  static any(signals: any): AbortSignal {
    const out = new AbortSignal();
    for (const src of signals) {
      if (src.aborted) { out.__doAbort(src.reason); return out; }
    }
    for (const src of signals) { __abort_follow(out, src); }
    return out;
  }
}

// Helper (not a class member): registering the propagator in a free function
// keeps the closure a plain param capture, not a per-iteration loop capture.
function __abort_follow(out: AbortSignal, src: AbortSignal): void {
  src.addEventListener("abort", () => { out.__doAbort(src.reason); });
}

class AbortController {
  signal: AbortSignal;
  constructor() { this.signal = new AbortSignal(); }
  abort(reason: any = undefined): void {
    if (reason === undefined) {
      this.signal.__doAbort(new DOMException("This operation was aborted", "AbortError"));
    } else {
      this.signal.__doAbort(reason);
    }
  }
}

class MessagePort {
  onmessage: any = null;
  // Wired by MessageChannel (double-underscore internal, same rationale as
  // AbortSignal.__doAbort).
  __peer: any = null;
  postMessage(data: any): void {
    const peer = this.__peer;
    queueMicrotask(() => {
      const h = peer.onmessage;
      if (h !== null && h !== undefined) { h({ data: data }); }
    });
  }
  start(): void {}
  close(): void {}
}

class MessageChannel {
  port1: MessagePort;
  port2: MessagePort;
  constructor() {
    this.port1 = new MessagePort();
    this.port2 = new MessagePort();
    this.port1.__peer = this.port2;
    this.port2.__peer = this.port1;
  }
}
