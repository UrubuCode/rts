// node:fs — ReadStream / WriteStream (and the createReadStream/createWriteStream
// factories). They EXTEND the ambient stream Readable/Writable, so `.pipe`,
// `instanceof stream.Readable`, backpressure, encoding and every inherited event
// come for free. The one thing a `.ts` prelude cannot do — touch the filesystem —
// is the private `engine.fs_*` bridge (real std::fs, in rts-node fs/streambridge.rs).
//
// The work is synchronous under the interim event loop (#207): a ReadStream reads
// the whole file once (deferred a microtask so listeners attach first) and pushes
// it as one chunk then EOF; a WriteStream writes each chunk straight through. All
// the async SHAPE (open/ready/data/end/close/finish/drain) is preserved.

class ReadStream extends Readable {
  path: any;
  bytesRead: any;
  pending: any;
  fd: any;
  __fsRead: any;
  constructor(path: any, options: any = undefined) {
    super(options);
    this.path = path;
    this.bytesRead = 0;
    this.pending = true;
    this.fd = null;
    this.__fsRead = false;
    const self: any = this;
    // `'open'`/`'ready'` fire once, asynchronously, after the caller attaches its
    // listeners; the file bytes stream lazily through `_read` (canonical Readable).
    queueMicrotask(() => {
      self.pending = false;
      self.fd = 0;
      self.emit("open", 0);
      self.emit("ready");
    });
  }
  // The Readable machinery calls this when it wants data (once flowing / on
  // `.read()` / `.pipe`). Read the whole file once, push it as one chunk, then
  // EOF. Flowing `on('data')`, `.pipe(dest)`, and `setEncoding` all deliver.
  // (A manual `on('end')` attached AFTER `on('data')` may miss `'end'` — the
  // synchronous-resume timing every stream shares; `.pipe` is unaffected.)
  _read(size: any): void {
    const self: any = this;
    if (self.__fsRead) { return; }
    self.__fsRead = true;
    let bytes: any = undefined;
    let failed: any = undefined;
    try { bytes = engine.fs_read_bytes(self.path); }
    catch (e) { failed = e; }
    if (failed !== undefined) {
      self.emit("error", failed);
      return;
    }
    self.bytesRead = bytes.length;
    self.push(bytes);
    self.push(null);
  }
  close(callback: any = undefined): void {
    const self: any = this;
    self.destroy();
    if (typeof callback === "function") { callback(); }
  }
}

function __fsWriteFlags(options: any): any {
  if (options && options.flags) {
    const f: any = options.flags;
    if (f === "a" || f === "a+" || f === "as" || f === "as+") { return true; }
  }
  return false;
}

class WriteStream extends Writable {
  path: any;
  bytesWritten: any;
  pending: any;
  fd: any;
  __fsAppend: any;
  __fsStarted: any;
  constructor(path: any, options: any = undefined) {
    super(options);
    this.path = path;
    this.bytesWritten = 0;
    this.pending = true;
    this.fd = null;
    this.__fsAppend = __fsWriteFlags(options);
    this.__fsStarted = false;
    const self: any = this;
    queueMicrotask(() => {
      self.pending = false;
      self.fd = 0;
      self.emit("open", 0);
      self.emit("ready");
    });
  }
  _write(chunk: any, encoding: any, cb: any): void {
    const self: any = this;
    let failed: any = undefined;
    try {
      if (self.__fsAppend || self.__fsStarted) {
        engine.fs_append_bytes(self.path, chunk);
      } else {
        engine.fs_write_bytes(self.path, chunk);
      }
      self.__fsStarted = true;
      const n: any = chunk.length;
      self.bytesWritten = self.bytesWritten + (n ? n : 0);
    } catch (e) {
      failed = e;
    }
    cb(failed);
  }
  close(callback: any = undefined): void {
    const self: any = this;
    self.end();
    if (typeof callback === "function") { callback(); }
  }
}

function createReadStream(path: any, options: any = undefined): any {
  return new ReadStream(path, options);
}

function createWriteStream(path: any, options: any = undefined): any {
  return new WriteStream(path, options);
}
