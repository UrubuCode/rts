// node:fs — `Utf8Stream`: a high-throughput, fixed-encoding, append-only file
// writer (Node's `fs.Utf8Stream`, the sonic-boom-style logger sink), distinct
// from `WriteStream`. Standalone (extends the stream event base only), backed by
// the real `engine.fs_*` file IO. Writes accumulate in a buffer and flush when it
// reaches `minLength` (0 = flush every write), on `flush()`/`flushSync()`/`end()`.
//
// The interim event loop is synchronous, so a flush writes straight through; the
// full async SHAPE (ready/write/drain/finish/close/error + buffering) is kept.

class Utf8Stream extends Stream {
  fd: any;
  file: any;
  append: any;
  mode: any;
  sync: any;
  fsync: any;
  mkdir: any;
  minLength: any;
  maxLength: any;
  periodicFlush: any;
  writing: any;
  contentMode: any;
  destroyed: any;
  __buf: any;
  __started: any;

  constructor(options: any = undefined) {
    super();
    const o: any = (options === undefined || options === null) ? {} : options;
    this.fd = (o.fd === undefined) ? null : o.fd;
    this.file = (o.dest === undefined) ? ((o.file === undefined) ? null : o.file) : o.dest;
    this.append = (o.append === false) ? false : true;
    this.mode = (o.mode === undefined) ? 438 : o.mode;
    this.sync = (o.sync === true) ? true : false;
    this.fsync = (o.fsync === true) ? true : false;
    this.mkdir = (o.mkdir === true) ? true : false;
    this.minLength = (o.minLength === undefined) ? 0 : o.minLength;
    this.maxLength = (o.maxLength === undefined) ? 0 : o.maxLength;
    this.periodicFlush = (o.periodicFlush === undefined) ? 0 : o.periodicFlush;
    this.contentMode = (o.contentMode === undefined) ? "utf8" : o.contentMode;
    this.writing = false;
    this.destroyed = false;
    // `!append` truncates on the first real write; until then nothing is written.
    this.__started = false;
    this.__buf = "";
    const self: any = this;
    queueMicrotask(() => { if (!self.destroyed) { self.emit("ready"); } });
  }

  write(data: any): any {
    const self: any = this;
    if (self.destroyed) {
      self.emit("error", new Error("the stream has been destroyed"));
      return false;
    }
    const s: any = "" + data;
    // maxLength: drop the write (emit 'drop') instead of unbounded growth.
    if (self.maxLength && (self.__buf.length + s.length) > self.maxLength) {
      self.emit("drop", data);
      return true;
    }
    self.__buf = self.__buf + s;
    if (self.__buf.length >= self.minLength) {
      __utf8FlushBuf(self);
    }
    // Backpressure hint: room left under minLength (Node returns false when full).
    return self.__buf.length < self.minLength;
  }

  flush(callback: any = undefined): void {
    const self: any = this;
    let failed: any = undefined;
    try { __utf8FlushBuf(self); }
    catch (e) { failed = e; }
    if (typeof callback === "function") { callback(failed); }
  }

  flushSync(): void { const self: any = this; __utf8FlushBuf(self); }

  reopen(file: any = undefined): void {
    const self: any = this;
    __utf8FlushBuf(self);
    if (file !== undefined && file !== null) { self.file = file; }
    // A reopened file restarts the truncate-vs-append decision.
    self.__started = self.append;
    self.emit("ready");
  }

  end(): void {
    const self: any = this;
    if (self.destroyed) { return; }
    __utf8FlushBuf(self);
    self.emit("finish");
    __utf8Close(self);
  }

  destroy(): void { const self: any = this; __utf8Close(self); }

  [Symbol.dispose](): void { const self: any = this; __utf8Close(self); }
}

// Flush the accumulated buffer to the file (truncate on the first write when not
// in append mode, append thereafter), emitting `'write'` with the byte count.
function __utf8FlushBuf(self: any): void {
  if (self.destroyed) { return; }
  if (self.__buf.length === 0) { return; }
  const path: any = self.file;
  if (path === null || path === undefined) { return; }
  const data: any = self.__buf;
  self.__buf = "";
  self.writing = true;
  if (self.append || self.__started) {
    engine.fs_append_bytes(path, data);
  } else {
    engine.fs_write_bytes(path, data);
  }
  self.__started = true;
  self.writing = false;
  self.emit("write", data.length);
  self.emit("drain");
}

function __utf8Close(self: any): void {
  if (self.destroyed) { return; }
  __utf8FlushBuf(self);
  self.destroyed = true;
  self.fd = null;
  self.emit("close");
}
