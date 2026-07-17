// node:stream — Duplex / Transform / PassThrough. Duplex extends Readable and
// mixes in the write side by initializing __w* state and exposing the Writable
// method surface (delegating to the same free functions writable.ts defines).
// Part of the node:stream ambient prelude; depends on readable.ts + writable.ts.

class Duplex extends Readable {
  allowHalfOpen: any = true;
  constructor(options: any = undefined) {
    super(options);
    this.__wIsDuplex = true;
    __wInit(this, options);
    if (options !== undefined && options !== null && (options.allowHalfOpen !== undefined && !options.allowHalfOpen)) {
      this.allowHalfOpen = false;
    }
    if (!this.allowHalfOpen) {
      const self = this;
      this.on("end", () => { if ((!self.__wEnded)) { self.end(); } });
    }
  }

  // ---- write-side surface (delegates to writable.ts free functions) ----
  _write(chunk: any, encoding: any, cb: any): void {
    throw __streamErr("ERR_METHOD_NOT_IMPLEMENTED", "The _write() method is not implemented");
  }
  _writev(chunks: any, cb: any): void {
    throw __streamErr("ERR_METHOD_NOT_IMPLEMENTED", "The _writev() method is not implemented");
  }
  _final(cb: any): void { cb(); }

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

// ==== Transform ============================================================
class Transform extends Duplex {
  __tTransforming: any = false;
  __userTransform: any = null;
  __userFlush: any = null;
  constructor(options: any = undefined) {
    super(options);
    // A Transform's readable side is driven by `_write`→`push`, never by `_read`
    // (Node installs an internal no-op `_read`); set it so a resumed Transform
    // does not hit the base "not implemented" read.
    this.__userRead = () => {};
    if (options !== undefined && options !== null) {
      if (typeof options.transform === "function") { this.__userTransform = options.transform; }
      if (typeof options.flush === "function") { this.__userFlush = options.flush; }
    }
  }

  _transform(chunk: any, encoding: any, cb: any): void {
    const fn = this.__userTransform;
    if (typeof fn === "function") { fn.call(this, chunk, encoding, cb); return; }
    throw __streamErr("ERR_METHOD_NOT_IMPLEMENTED", "The _transform() method is not implemented");
  }
  _flush(cb: any): void {
    const fn = this.__userFlush;
    if (typeof fn === "function") { fn.call(this, cb); return; }
    cb();
  }

  // Writable hook: run the transform, push its output onto the readable side.
  _write(chunk: any, encoding: any, cb: any): void {
    const self = this;
    this.__tTransforming = true;
    const after = (err: any, data: any) => {
      self.__tTransforming = false;
      if (data !== undefined && data !== null) { __push(self, data, undefined, false); }
      cb(err === undefined ? null : err);
    };
    this._transform(chunk, encoding, after);
  }

  // Writable final: flush trailing output, then end the readable side.
  _final(cb: any): void {
    const self = this;
    this._flush((err: any, data: any) => {
      if (data !== undefined && data !== null) { __push(self, data, undefined, false); }
      __push(self, null, undefined, false);
      cb(err === undefined ? null : err);
    });
  }
}

// ==== PassThrough ==========================================================
class PassThrough extends Transform {
  constructor(options: any = undefined) {
    super(options);
    // Identity transform. Set as __userTransform too so the base Transform._write
    // path (`this._transform`) forwards unchanged even if virtual dispatch resolves
    // to Transform._transform rather than this override.
    this.__userTransform = (chunk: any, encoding: any, cb: any) => { cb(null, chunk); };
  }
  _transform(chunk: any, encoding: any, cb: any): void { cb(null, chunk); }
}
