// node:stream — the read side. Shared by Readable AND Duplex via free functions
// taking the stream as `self` (so Duplex reuses the exact same code path). Part
// of the node:stream ambient prelude; depends on stream.ts (NodeEmitter/Stream)
// and on the ambient `__utf8_decode`/`Buffer` from earlier preludes.

function __rInit(self: any, opts: any): void {
  const o = opts === undefined || opts === null ? undefined : opts;
  self.__rBuffer = [];
  self.__rLength = 0;
  self.__rFlowState = 0;
  self.__rEnded = false;
  self.__rEndEmitted = false;
  self.__rReadableState = true;
  self.__rDestroyed = false;
  self.__rClosed = false;
  self.__rErrored = null;
  self.__rDidRead = false;
  self.__rReading = false;
  self.__rPipes = [];
  self.__rPipeHandlers = [];
  self.__rEncoding = null;
  self.__rObjectMode = false;
  self.__rHWM = 16384;
  self.__rAutoDestroy = true;
  self.__rEmitClose = true;
  self.__userRead = null;
  self.__userDestroy = null;
  self.__userConstruct = null;
  if (o !== undefined) {
    if (o.objectMode || o.readableObjectMode) { self.__rObjectMode = true; }
    self.__rHWM = self.__rObjectMode ? 16 : 16384;
    if (typeof o.highWaterMark === "number") { self.__rHWM = o.highWaterMark; }
    if (typeof o.readableHighWaterMark === "number") { self.__rHWM = o.readableHighWaterMark; }
    if (typeof o.encoding === "string") {
      if (!__validEncoding(o.encoding)) { throw __streamErr("ERR_UNKNOWN_ENCODING", "Unknown encoding: " + o.encoding); }
      self.__rEncoding = o.encoding;
    }
    if ((o.autoDestroy !== undefined && !o.autoDestroy)) { self.__rAutoDestroy = false; }
    if ((o.emitClose !== undefined && !o.emitClose)) { self.__rEmitClose = false; }
    if (typeof o.read === "function") { self.__userRead = o.read; }
    if (typeof o.destroy === "function") { self.__userDestroy = o.destroy; }
    if (typeof o.construct === "function") { self.__userConstruct = o.construct; }
  }
}

function __rDecode(self: any, chunk: any): any {
  const enc = self.__rEncoding;
  if (enc === null || typeof chunk === "string") { return chunk; }
  if (chunk === null || chunk === undefined) { return chunk; }
  return __utf8_decode(chunk);
}

// Deliver every buffered chunk while in flowing mode; then end if drained.
function __rFlow(self: any): void {
  while (self.__rFlowState === 1 && self.__rBuffer.length > 0) {
    const chunk = self.__rBuffer.shift();
    self.__rLength -= __chunkSize(chunk, self.__rObjectMode);
    self.__rDidRead = true;
    self.emit("data", __rDecode(self, chunk));
  }
  __rEndIfDone(self);
}

function __rEndIfDone(self: any): void {
  if (self.__rEnded && (!self.__rEndEmitted) && self.__rBuffer.length === 0) {
    self.__rEndEmitted = true;
    self.__rReadableState = false;
    self.emit("end");
    if (self.__rAutoDestroy && !self.__wIsDuplex) { __destroy(self, null); }
  }
}

function __push(self: any, chunk: any, enc: any, front: any): any {
  if (self.__rEnded && chunk !== null) {
    self.emit("error", __streamErr("ERR_STREAM_PUSH_AFTER_EOF", "stream.push() after EOF"));
    return false;
  }
  if (chunk === null) {
    self.__rEnded = true;
    if (self.__rFlowState === 1) { __rFlow(self); } else { self.emit("readable"); __rEndIfDone(self); }
    return false;
  }
  if (chunk === undefined) { return self.__rLength < self.__rHWM; }
  if (front) { self.__rBuffer.unshift(chunk); } else { self.__rBuffer.push(chunk); }
  self.__rLength += __chunkSize(chunk, self.__rObjectMode);
  self.__rReadableState = true;
  if (self.__rFlowState === 1) { __rFlow(self); }
  else { self.emit("readable"); }
  return self.__rLength < self.__rHWM;
}

function __rResume(self: any): void {
  if (self.__rFlowState === 1) { return; }
  self.__rFlowState = 1;
  self.emit("resume");
  // Drain synchronously (see stream.ts deviation note): RTS's test harness does
  // not drain microtasks between top-level setup and assertions, so a deferred
  // (nextTick) drain would never run in time. Consumers therefore attach all
  // listeners before the data becomes available (the standard paused→resume
  // pattern), or push after attaching in flowing mode.
  __rRead0(self);
  __rFlow(self);
}

// Ask the implementation for more data (calls _read once, re-entrancy guarded).
function __rRead0(self: any): void {
  if (self.__rReading || self.__rEnded || self.__rDestroyed) { return; }
  if (self.__rLength >= self.__rHWM && self.__rLength > 0) { return; }
  self.__rReading = true;
  const fn = self.__userRead;
  if (typeof fn === "function") { fn.call(self, self.__rHWM); }
  else { self._read(self.__rHWM); }
  self.__rReading = false;
}

// Boolean-heavy read logic lives in free functions: synthesized class METHODS
// (`__rtsn_method_*`) reject a Tagged value in a bool-typed context (``,
// `x || y`, `!x`), while free functions taking `self: any` handle it fine — so
// every method with such logic delegates here.
function __rReadImpl(self: any, size: any): any {
  const objMode = self.__rObjectMode;
  if (self.__rBuffer.length === 0) { __rRead0(self); }
  if (self.__rBuffer.length === 0) { __rEndIfDone(self); return null; }
  if (objMode || size === undefined || size === null) {
    const chunk = self.__rBuffer.shift();
    self.__rLength -= __chunkSize(chunk, objMode);
    self.__rDidRead = true;
    __rEndIfDone(self);
    return __rDecode(self, chunk);
  }
  const want = size;
  if (self.__rLength < want && !self.__rEnded) { __rRead0(self); }
  if (self.__rLength < want && !self.__rEnded) { return null; }
  const first = self.__rBuffer[0];
  if (typeof first === "string") {
    let acc = "";
    while (acc.length < want && self.__rBuffer.length > 0) { acc += self.__rBuffer.shift(); }
    const out = acc.slice(0, want);
    const rest = acc.slice(want);
    self.__rLength -= acc.length;
    if (rest.length > 0) { self.__rBuffer.unshift(rest); self.__rLength += rest.length; }
    self.__rDidRead = true;
    __rEndIfDone(self);
    return out;
  }
  const parts: any[] = [];
  let got = 0;
  while (got < want && self.__rBuffer.length > 0) {
    const c = self.__rBuffer.shift();
    parts.push(c);
    got += __chunkSize(c, false);
  }
  self.__rLength -= got;
  const merged = Buffer.concat(parts);
  const outB = merged.slice(0, want);
  if (merged.length > want) { const rest = merged.slice(want); self.__rBuffer.unshift(rest); self.__rLength += rest.length; }
  self.__rDidRead = true;
  __rEndIfDone(self);
  return self.__rEncoding !== null ? __rDecode(self, outB) : outB;
}

function __rPause(self: any): void {
  if (self.__rFlowState !== 2) { self.__rFlowState = 2; self.emit("pause"); }
}
function __rIsPaused(self: any): any { return self.__rFlowState === 2; }
function __rIsReadable(self: any): any {
  return self.__rReadableState && !self.__rDestroyed && !self.__rEndEmitted;
}
function __rIsAborted(self: any): any {
  return self.__rDestroyed && !self.__rEndEmitted;
}
function __rUnshift(self: any, chunk: any, enc: any): any {
  if (self.__rEndEmitted) { throw __streamErr("ERR_STREAM_UNSHIFT_AFTER_END_EVENT", "stream.unshift() after end event"); }
  return __push(self, chunk, enc, true);
}

// A registered pipe target + its wired listeners. A REAL class, not an object
// literal: an object literal with function-valued props (`onData`/…) synthesizes
// a `__fnprop_N` whose GLOBAL numbering would collide with the host program's
// own object-literal methods (this prelude merges into every program).
class __PipeH {
  dest: any = null;
  onData: any = null;
  onDrain: any = null;
  onEnd: any = null;
}

// ==== Readable =============================================================
class Readable extends Stream {
  __wIsDuplex: any = false;
  constructor(options: any = undefined) {
    super();
    __rInit(this, options);
  }

  _read(size: any): void {
    throw __streamErr("ERR_METHOD_NOT_IMPLEMENTED", "The _read() method is not implemented");
  }
  _construct(cb: any): void { cb(); }
  _destroy(err: any, cb: any): void { cb(err); }

  // Attaching a 'data' listener switches the stream into flowing mode (Node
  // semantics) — do it here so `r.on('data', …)` starts delivery with no
  // explicit `.resume()`.
  on(name: any, fn: any): any {
    const r = super.on(name, fn);
    if (name === "data") { __rResume(this); }
    return r;
  }
  addListener(name: any, fn: any): any { return this.on(name, fn); }

  push(chunk: any, encoding: any = undefined): any { return __push(this, chunk, encoding, false); }
  unshift(chunk: any, encoding: any = undefined): any { return __rUnshift(this, chunk, encoding); }

  read(size: any = undefined): any { return __rReadImpl(this, size); }

  pause(): any { __rPause(this); return this; }
  resume(): any { __rResume(this); return this; }
  isPaused(): any { return __rIsPaused(this); }

  setEncoding(enc: string): any {
    if (!__validEncoding(enc)) { throw __streamErr("ERR_UNKNOWN_ENCODING", "Unknown encoding: " + enc); }
    this.__rEncoding = enc;
    return this;
  }

  pipe(dest: any, opts: any = undefined): any {
    const self = this;
    const endDest = !(opts !== undefined && opts !== null && (opts.end !== undefined && !opts.end));
    this.__rPipes.push(dest);
    dest.emit("pipe", this);
    const onData = (chunk: any) => {
      const ok = dest.write(chunk);
      if ((ok !== undefined && !ok)) { self.pause(); }
    };
    const onDrain = () => { self.resume(); };
    const onEnd = () => { if (endDest) { dest.end(); } };
    // Attach 'end'/'drain' BEFORE 'data': attaching the 'data' listener switches
    // the source to flowing mode and (with synchronous drain) delivers any
    // already-buffered chunks + the 'end' immediately, so 'end' must be wired
    // first or `dest.end()` would be missed.
    dest.on("drain", onDrain);
    this.on("end", onEnd);
    const h = new __PipeH();
    h.dest = dest;
    h.onData = onData;
    h.onDrain = onDrain;
    h.onEnd = onEnd;
    this.__rPipeHandlers.push(h);
    this.on("data", onData);
    return dest;
  }
  unpipe(dest: any = undefined): any {
    const handlers = this.__rPipeHandlers;
    const keep: any[] = [];
    const keptPipes: any[] = [];
    for (let i = 0; i < handlers.length; i++) {
      const h = handlers[i];
      if (dest === undefined || h.dest === dest) {
        this.off("data", h.onData);
        this.off("end", h.onEnd);
        h.dest.off("drain", h.onDrain);
        h.dest.emit("unpipe", this);
      } else {
        keep.push(h);
        keptPipes.push(h.dest);
      }
    }
    this.__rPipeHandlers = keep;
    this.__rPipes = keptPipes;
    return this;
  }
  wrap(old: any): any {
    const self = this;
    old.on("data", (chunk: any) => { __push(self, chunk, undefined, false); });
    old.on("end", () => { __push(self, null, undefined, false); });
    old.on("error", (e: any) => { self.destroy(e); });
    self._read = () => { if (typeof old.resume === "function") { old.resume(); } };
    return this;
  }

  destroy(err: any = undefined): any { __destroy(this, err === undefined ? null : err); return this; }

  get closed(): any { return this.__rClosed; }
  get destroyed(): any { return this.__rDestroyed; }
  get errored(): any { return this.__rErrored; }
  get readable(): any { return __rIsReadable(this); }
  get readableEnded(): any { return this.__rEndEmitted; }
  get readableFlowing(): any { const s = this.__rFlowState; if (s === 1) { return true; } if (s === 2) { return false; } return null; }
  get readableHighWaterMark(): any { return this.__rHWM; }
  get readableLength(): any { return this.__rLength; }
  get readableObjectMode(): any { return this.__rObjectMode; }
  get readableEncoding(): any { return this.__rEncoding; }
  get readableAborted(): any { return __rIsAborted(this); }
  get readableDidRead(): any { return this.__rDidRead; }

  // ---- async-iteration helpers (v17+) — eager-drain semantics -----------
  // RTS's engine await is synchronous-passthrough and this module's flow is
  // synchronous, so these drain fully then compute; results match Node.
  toArray(opts: any = undefined): any { return Promise.resolve(__rDrain(this)); }
  forEach(fn: any, opts: any = undefined): any {
    const all = __rDrain(this);
    for (let i = 0; i < all.length; i++) { fn(all[i]); }
    return Promise.resolve(undefined);
  }
  map(fn: any, opts: any = undefined): any {
    const all = __rDrain(this);
    const out: any[] = [];
    for (let i = 0; i < all.length; i++) { out.push(fn(all[i], i)); }
    return __from(out);
  }
  filter(fn: any, opts: any = undefined): any {
    const all = __rDrain(this);
    const out: any[] = [];
    for (let i = 0; i < all.length; i++) { if (fn(all[i], i)) { out.push(all[i]); } }
    return __from(out);
  }
  flatMap(fn: any, opts: any = undefined): any {
    const all = __rDrain(this);
    const out: any[] = [];
    for (let i = 0; i < all.length; i++) {
      const r = fn(all[i], i);
      if (Array.isArray(r)) { for (let j = 0; j < r.length; j++) { out.push(r[j]); } }
      else { out.push(r); }
    }
    return __from(out);
  }
  drop(n: any, opts: any = undefined): any {
    const all = __rDrain(this);
    return __from(all.slice(n));
  }
  take(n: any, opts: any = undefined): any {
    const all = __rDrain(this);
    return __from(all.slice(0, n));
  }
  reduce(fn: any, initial: any = undefined, opts: any = undefined): any {
    const all = __rDrain(this);
    let acc = initial;
    let i = 0;
    if (initial === undefined) {
      if (all.length === 0) { throw __streamErr("ERR_INVALID_ARG_VALUE", "Reduce of empty stream with no initial value"); }
      acc = all[0]; i = 1;
    }
    for (; i < all.length; i++) { acc = fn(acc, all[i]); }
    return Promise.resolve(acc);
  }
  some(fn: any, opts: any = undefined): any {
    const all = __rDrain(this);
    for (let i = 0; i < all.length; i++) { if (fn(all[i], i)) { return Promise.resolve(true); } }
    return Promise.resolve(false);
  }
  every(fn: any, opts: any = undefined): any {
    const all = __rDrain(this);
    for (let i = 0; i < all.length; i++) { if (fn(all[i], i)) { } else { return Promise.resolve(false); } }
    return Promise.resolve(true);
  }
  find(fn: any, opts: any = undefined): any {
    const all = __rDrain(this);
    for (let i = 0; i < all.length; i++) { if (fn(all[i], i)) { return Promise.resolve(all[i]); } }
    return Promise.resolve(undefined);
  }

  static from(iterable: any, options: any = undefined): any { return __from(iterable, options); }
  static isDisturbed(s: any): any { return s !== null && s !== undefined && (s.__rDidRead || s.__rErrored !== null); }
}

// Drain a Readable synchronously into an array (flowing mode → 'end').
function __rDrain(self: any): any[] {
  const out: any[] = [];
  self.on("data", (c: any) => { out.push(c); });
  self.resume();
  return out;
}

// Build a Readable from an array / iterable / iterator (objectMode). Eager.
function __from(iterable: any, options: any = undefined): any {
  const r = new Readable({ objectMode: true });
  if (iterable !== undefined && iterable !== null) {
    if (Array.isArray(iterable)) {
      for (let i = 0; i < iterable.length; i++) { __push(r, iterable[i], undefined, false); }
    } else if (typeof iterable.next === "function") {
      let n = iterable.next();
      while (n !== undefined && !n.done) { __push(r, n.value, undefined, false); n = iterable.next(); }
    } else {
      for (const it of iterable) { __push(r, it, undefined, false); }
    }
  }
  __push(r, null, undefined, false);
  return r;
}

// Shared destroy (read/write/duplex). Emits 'error' (if any) then 'close'.
function __destroy(self: any, err: any): void {
  if (self.__rDestroyed || self.__wDestroyed) { return; }
  self.__rDestroyed = true;
  self.__wDestroyed = true;
  self.__rReadableState = false;
  if (err !== null && err !== undefined) {
    self.__rErrored = err;
    if (self.__wSetErrored !== undefined) { self.__wSetErrored(err); }
  }
  const done = (e: any) => {
    if (e !== null && e !== undefined) { self.emit("error", e); }
    else if (err !== null && err !== undefined) { self.emit("error", err); }
    const emitClose = self.__rEmitClose;
    if (emitClose) { self.__rClosed = true; self.__wClosed = true; self.emit("close"); }
  };
  const ud = self.__userDestroy;
  if (typeof ud === "function") { ud.call(self, err, done); }
  else { self._destroy(err, done); }
}
