// node:stream — the write side. Shared by Writable AND Duplex via free functions
// taking the stream as `self`. Part of the node:stream ambient prelude; depends
// on stream.ts (NodeEmitter/Stream) and readable.ts (__destroy/__streamErr).

// A queued write. A REAL class (not an object literal): an object literal with
// function-valued properties (`cb`) synthesizes a `__fnprop_N` helper whose
// GLOBAL numbering would collide with the host program's own object-literal
// methods (this prelude is merged into EVERY program).
class __WItem {
  chunk: any = undefined;
  encoding: any = undefined;
  cb: any = undefined;
  size: any = 0;
}

function __wInit(self: any, opts: any): void {
  const o = opts === undefined || opts === null ? undefined : opts;
  self.__wBuffer = [];
  self.__wLength = 0;
  self.__wHWM = 16384;
  self.__wObjectMode = false;
  self.__wEnded = false;
  self.__wFinished = false;
  self.__wDestroyed = false;
  self.__wClosed = false;
  self.__wErrored = null;
  self.__wCorked = 0;
  self.__wNeedDrain = false;
  self.__wWriting = false;
  self.__wFinalCalled = false;
  self.__wDefaultEncoding = "utf8";
  self.__wDecodeStrings = true;
  self.__wAutoDestroy = true;
  self.__wEmitClose = true;
  self.__userWrite = null;
  self.__userWritev = null;
  self.__userFinal = null;
  if (o !== undefined) {
    if (o.objectMode || o.writableObjectMode) { self.__wObjectMode = true; }
    self.__wHWM = self.__wObjectMode ? 16 : 16384;
    if (typeof o.highWaterMark === "number") { self.__wHWM = o.highWaterMark; }
    if (typeof o.writableHighWaterMark === "number") { self.__wHWM = o.writableHighWaterMark; }
    if (typeof o.defaultEncoding === "string") {
      if (!__validEncoding(o.defaultEncoding)) { throw __streamErr("ERR_UNKNOWN_ENCODING", "Unknown encoding: " + o.defaultEncoding); }
      self.__wDefaultEncoding = o.defaultEncoding;
    }
    if ((o.decodeStrings !== undefined && !o.decodeStrings)) { self.__wDecodeStrings = false; }
    if ((o.autoDestroy !== undefined && !o.autoDestroy)) { self.__wAutoDestroy = false; }
    if ((o.emitClose !== undefined && !o.emitClose)) { self.__wEmitClose = false; }
    if (typeof o.write === "function") { self.__userWrite = o.write; }
    if (typeof o.writev === "function") { self.__userWritev = o.writev; }
    if (typeof o.final === "function") { self.__userFinal = o.final; }
    if (typeof o.destroy === "function") { self.__userDestroy = o.destroy; }
  }
  self.__wSetErrored = (e: any) => { self.__wErrored = e; };
}

function __wCallWrite(self: any, chunk: any, enc: any, cb: any): void {
  const fn = self.__userWrite;
  if (typeof fn === "function") { fn.call(self, chunk, enc, cb); }
  else { self._write(chunk, enc, cb); }
}

// Flush the buffered writes (respecting cork). Uses _writev when the user
// provided it and more than one chunk is queued (the corked batch path).
function __wProcess(self: any): void {
  if (self.__wCorked > 0 || self.__wWriting) { return; }
  if (self.__wBuffer.length === 0) { return; }
  self.__wWriting = true;
  if (self.__userWritev !== null && self.__wBuffer.length > 1) {
    const batch = self.__wBuffer;
    self.__wBuffer = [];
    const items: any[] = [];
    let total = 0;
    for (let i = 0; i < batch.length; i++) {
      items.push({ chunk: batch[i].chunk, encoding: batch[i].encoding });
      total += batch[i].size;
    }
    const cbs = batch;
    const done = (err: any) => {
      self.__wLength -= total;
      for (let i = 0; i < cbs.length; i++) { if (typeof cbs[i].cb === "function") { cbs[i].cb(err === undefined ? null : err); } }
      self.__wWriting = false;
      __wAfterWrite(self, err);
    };
    self.__userWritev.call(self, items, done);
    return;
  }
  const item = self.__wBuffer.shift();
  const done = (err: any) => {
    self.__wLength -= item.size;
    if (typeof item.cb === "function") { item.cb(err === undefined ? null : err); }
    self.__wWriting = false;
    __wAfterWrite(self, err);
  };
  __wCallWrite(self, item.chunk, item.encoding, done);
}

function __wAfterWrite(self: any, err: any): void {
  if (err !== null && err !== undefined) { __destroy(self, err); return; }
  if (self.__wBuffer.length > 0) { __wProcess(self); return; }
  if (self.__wNeedDrain && self.__wLength < self.__wHWM) {
    self.__wNeedDrain = false;
    self.emit("drain");
  }
  if (self.__wEnded && (!self.__wFinished) && self.__wLength === 0) { __wFinish(self); }
}

function __wFinish(self: any): void {
  if (self.__wFinished || self.__wFinalCalled) { return; }
  self.__wFinalCalled = true;
  const emitFinish = () => {
    self.__wFinished = true;
    self.emit("finish");
    if (self.__wAutoDestroy && !self.__wIsDuplex) { __destroy(self, null); }
  };
  const uf = self.__userFinal;
  if (typeof uf === "function") { uf.call(self, (err: any) => { if (err !== null && err !== undefined) { __destroy(self, err); } else { emitFinish(); } }); }
  else { emitFinish(); }
}

function __wDoWrite(self: any, chunk: any, enc: any, cb: any): any {
  if (self.__wEnded) {
    const e = __streamErr("ERR_STREAM_WRITE_AFTER_END", "write after end");
    if (typeof cb === "function") { cb(e); }
    self.emit("error", e);
    return false;
  }
  if (chunk === null) { throw __streamErr("ERR_STREAM_NULL_VALUES", "May not write null values to stream"); }
  if (self.__wDestroyed) {
    const e = __streamErr("ERR_STREAM_DESTROYED", "Cannot call write after a stream was destroyed");
    if (typeof cb === "function") { cb(e); }
    return false;
  }
  const encoding = typeof enc === "string" ? enc : self.__wDefaultEncoding;
  const size = __chunkSize(chunk, self.__wObjectMode);
  const it = new __WItem();
  it.chunk = chunk;
  it.encoding = encoding;
  it.cb = cb;
  it.size = size;
  self.__wBuffer.push(it);
  self.__wLength += size;
  __wProcess(self);
  const ok = self.__wLength < self.__wHWM;
  if (!ok) { self.__wNeedDrain = true; }
  return ok;
}

// Free functions for the boolean-heavy bits (methods can't coerce Tagged→Bool).
function __wFinishIfIdle(self: any): void {
  if (self.__wWriting) { return; }
  if (self.__wLength === 0) { __wFinish(self); }
}
function __wEndImpl(self: any, chunk: any, encoding: any, cb: any): void {
  let data = chunk;
  let enc = encoding;
  let done = cb;
  if (typeof chunk === "function") { done = chunk; data = undefined; enc = undefined; }
  else if (typeof encoding === "function") { done = encoding; enc = undefined; }
  if (typeof done === "function") { self.once("finish", done); }
  if (data !== undefined && data !== null) { __wDoWrite(self, data, enc, undefined); }
  self.__wEnded = true;
  __wFinishIfIdle(self);
}
function __wIsWritable(self: any): any { return !self.__wEnded && !self.__wDestroyed; }
function __wIsAborted(self: any): any { return self.__wDestroyed && !self.__wFinished; }

// ==== Writable =============================================================
class Writable extends Stream {
  __wIsDuplex: any = false;
  constructor(options: any = undefined) {
    super();
    __wInit(this, options);
  }

  _write(chunk: any, encoding: any, cb: any): void {
    throw __streamErr("ERR_METHOD_NOT_IMPLEMENTED", "The _write() method is not implemented");
  }
  _writev(chunks: any, cb: any): void {
    throw __streamErr("ERR_METHOD_NOT_IMPLEMENTED", "The _writev() method is not implemented");
  }
  _final(cb: any): void { cb(); }
  _construct(cb: any): void { cb(); }
  _destroy(err: any, cb: any): void { cb(err); }

  write(chunk: any, encoding: any = undefined, cb: any = undefined): any {
    let enc = encoding;
    let done = cb;
    if (typeof encoding === "function") { done = encoding; enc = undefined; }
    return __wDoWrite(this, chunk, enc, done);
  }

  end(chunk: any = undefined, encoding: any = undefined, cb: any = undefined): any {
    __wEndImpl(this, chunk, encoding, cb);
    return this;
  }

  cork(): void { this.__wCorked += 1; }
  uncork(): void {
    if (this.__wCorked > 0) { this.__wCorked -= 1; }
    if (this.__wCorked === 0) { __wProcess(this); }
  }
  setDefaultEncoding(enc: string): any {
    if (!__validEncoding(enc)) { throw __streamErr("ERR_UNKNOWN_ENCODING", "Unknown encoding: " + enc); }
    this.__wDefaultEncoding = enc;
    return this;
  }
  destroy(err: any = undefined): any { __destroy(this, err === undefined ? null : err); return this; }

  get closed(): any { return this.__wClosed; }
  get destroyed(): any { return this.__wDestroyed; }
  get errored(): any { return this.__wErrored; }
  get writable(): any { return __wIsWritable(this); }
  get writableEnded(): any { return this.__wEnded; }
  get writableFinished(): any { return this.__wFinished; }
  get writableHighWaterMark(): any { return this.__wHWM; }
  get writableLength(): any { return this.__wLength; }
  get writableObjectMode(): any { return this.__wObjectMode; }
  get writableCorked(): any { return this.__wCorked; }
  get writableNeedDrain(): any { return this.__wNeedDrain; }
  get writableAborted(): any { return __wIsAborted(this); }
}