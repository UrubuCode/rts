// node:stream — core classes (ambient `.ts` prelude, NOT primordial). Pure
// state-machine over primordial Array/Map/Buffer/Promise, mirroring Node's
// lib/internal/streams/*. Read-side state lives in `__r*` fields, write-side in
// `__w*` fields, so `Duplex` can hold BOTH without prototype mixing. The shared
// logic is free functions taking the stream as `self` (Readable/Duplex share the
// same read code; Writable/Duplex share the same write code).
//
// Deviation (documented): completion events ('end'/'finish'/'drain'/'close')
// are emitted synchronously rather than deferred via process.nextTick — RTS's
// engine await is synchronous-passthrough, so deterministic in-order emission
// matches assertion-style tests better than a deferred queue would. Ordering
// guarantees between events are preserved.

// ---- default highWaterMark (process-wide, mutable) ------------------------
let __nodeHwmByte: any = 16384;
let __nodeHwmObj: any = 16;

function getDefaultHighWaterMark(objectMode: any): any {
  return objectMode ? __nodeHwmObj : __nodeHwmByte;
}
function setDefaultHighWaterMark(objectMode: any, value: any): void {
  if (objectMode) { __nodeHwmObj = value; } else { __nodeHwmByte = value; }
}

// ---- error helper: an Error carrying a Node `.code` -----------------------
function __streamErr(code: string, msg: string): any {
  const e: any = new Error(msg);
  e.code = code;
  return e;
}

// A recognized BufferEncoding (used to validate setEncoding/write encoding).
function __validEncoding(enc: string): any {
  return enc === "utf8" || enc === "utf-8" || enc === "ascii" ||
    enc === "utf16le" || enc === "utf-16le" || enc === "ucs2" || enc === "ucs-2" ||
    enc === "base64" || enc === "base64url" || enc === "latin1" ||
    enc === "binary" || enc === "hex";
}

// Byte/object length of a chunk.
function __chunkSize(chunk: any, objectMode: any): any {
  if (objectMode) { return 1; }
  if (typeof chunk === "string") { return chunk.length; }
  if (chunk !== null && chunk !== undefined && typeof chunk.length === "number") {
    return chunk.length;
  }
  return 1;
}

// ---- EventEmitter base (self-contained; not the native EventEmitter) -------
class NodeEmitter {
  __ev: Map<string, any[]> = new Map();
  __maxListeners: any = 10;

  on(name: string, fn: any): any {
    let arr = this.__ev.get(name);
    if (arr === undefined) { arr = []; this.__ev.set(name, arr); }
    arr.push(fn);
    return this;
  }
  addListener(name: string, fn: any): any { return this.on(name, fn); }
  prependListener(name: string, fn: any): any {
    let arr = this.__ev.get(name);
    if (arr === undefined) { arr = []; this.__ev.set(name, arr); }
    arr.unshift(fn);
    return this;
  }
  once(name: string, fn: any): any {
    const self = this;
    let fired = false;
    const wrap = (a: any, b: any) => {
      if (fired) { return; }
      fired = true;
      self.off(name, wrap);
      fn(a, b);
    };
    return this.on(name, wrap);
  }
  prependOnceListener(name: string, fn: any): any {
    const self = this;
    let fired = false;
    const wrap = (a: any, b: any) => {
      if (fired) { return; }
      fired = true;
      self.off(name, wrap);
      fn(a, b);
    };
    return this.prependListener(name, wrap);
  }
  off(name: string, fn: any): any {
    const arr = this.__ev.get(name);
    if (arr !== undefined) {
      const out: any[] = [];
      for (let i = 0; i < arr.length; i++) {
        if (arr[i] !== fn) { out.push(arr[i]); }
      }
      this.__ev.set(name, out);
    }
    return this;
  }
  removeListener(name: string, fn: any): any { return this.off(name, fn); }
  removeAllListeners(name: any = undefined): any {
    if (name === undefined) { this.__ev = new Map(); }
    else { this.__ev.set(name, []); }
    return this;
  }
  listeners(name: string): any[] {
    const arr = this.__ev.get(name);
    return arr === undefined ? [] : arr.slice(0);
  }
  rawListeners(name: string): any[] { return this.listeners(name); }
  listenerCount(name: string): any {
    const arr = this.__ev.get(name);
    return arr === undefined ? 0 : arr.length;
  }
  eventNames(): any[] {
    const out: any[] = [];
    for (const k of this.__ev.keys()) {
      if (this.listenerCount(k) > 0) { out.push(k); }
    }
    return out;
  }
  setMaxListeners(n: any): any { this.__maxListeners = n; return this; }
  getMaxListeners(): any { return this.__maxListeners; }

  emit(name: string, a: any = undefined, b: any = undefined): any {
    const arr = this.__ev.get(name);
    if (arr === undefined || arr.length === 0) {
      if (name === "error") { throw a; }
      return false;
    }
    const snap = arr.slice(0);
    for (let i = 0; i < snap.length; i++) { snap[i](a, b); }
    return true;
  }
}

// The legacy base class (`instanceof stream.Stream`). Every concrete stream is
// also a Stream; RTS gives it no behavior beyond being the ancestor.
class Stream extends NodeEmitter {}
