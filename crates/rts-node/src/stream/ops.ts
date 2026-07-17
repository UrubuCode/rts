// node:stream — orchestration functions + stream/consumers + stream/promises,
// as ambient prelude declarations. Depends on the class files above.

function __isStream(x: any): any {
  if (x === null || x === undefined) { return false; }
  if (typeof x.pipe === "function") { return true; }
  if (typeof x.on === "function" && (typeof x.write === "function" || typeof x.read === "function")) { return true; }
  return false;
}

function __toStream(x: any): any {
  if (__isStream(x)) { return x; }
  if (Array.isArray(x) || (x !== null && x !== undefined && typeof x.next === "function")) { return __from(x); }
  return x;
}

function __abortErr(): any {
  const e: any = __streamErr("ABORT_ERR", "The operation was aborted");
  e.name = "AbortError";
  return e;
}

// ---- pipeline (callback form) --------------------------------------------
function pipeline(a1: any, a2: any = undefined, a3: any = undefined, a4: any = undefined, a5: any = undefined, a6: any = undefined, a7: any = undefined, a8: any = undefined): any {
  let list: any[] = [];
  let cb: any = undefined;
  if (Array.isArray(a1)) {
    list = a1.slice(0);
    cb = a2;
  } else {
    const raw = [a1, a2, a3, a4, a5, a6, a7, a8];
    for (let i = 0; i < raw.length; i++) { if (raw[i] !== undefined) { list.push(raw[i]); } }
    if (list.length > 0 && typeof list[list.length - 1] === "function" && !__isStream(list[list.length - 1])) {
      cb = list.pop();
    }
  }
  const streams: any[] = [];
  for (let i = 0; i < list.length; i++) { streams.push(__toStream(list[i])); }

  let called = false;
  const finish = (err: any) => {
    if (called) { return; }
    called = true;
    if (err !== null && err !== undefined) {
      for (let i = 0; i < streams.length; i++) { if (typeof streams[i].destroy === "function") { streams[i].destroy(err); } }
    }
    if (typeof cb === "function") { cb(err === undefined ? null : err); }
  };

  for (let i = 0; i < streams.length; i++) {
    const s = streams[i];
    if (typeof s.on === "function") { s.on("error", (e: any) => { finish(e); }); }
  }
  for (let i = 0; i < streams.length - 1; i++) {
    if (typeof streams[i].pipe === "function") { streams[i].pipe(streams[i + 1]); }
  }
  const last = streams.length > 0 ? streams[streams.length - 1] : undefined;
  if (last !== undefined && typeof last.on === "function") {
    last.on("finish", () => { finish(null); });
    last.on("end", () => { finish(null); });
    last.on("close", () => { finish(null); });
  }
  return last;
}

// ---- finished (callback form) --------------------------------------------
function finished(stream: any, a2: any = undefined, a3: any = undefined): any {
  let opts: any = a2;
  let cb: any = a3;
  if (typeof a2 === "function") { cb = a2; opts = undefined; }
  let called = false;
  const done = (err: any) => {
    if (called) { return; }
    called = true;
    if (typeof cb === "function") { cb(err === undefined ? null : err); }
  };
  const onEnd = () => { done(null); };
  const onFinish = () => { done(null); };
  const onClose = () => { done(null); };
  const onError = (e: any) => { done(e); };
  stream.on("end", onEnd);
  stream.on("finish", onFinish);
  stream.on("close", onClose);
  stream.on("error", onError);
  // already-finished / already-errored streams settle immediately
  if (stream.errored !== undefined && stream.errored !== null) { done(stream.errored); }
  else if (stream.readableEnded || stream.writableFinished || stream.closed) { done(null); }
  const cleanup = () => {
    stream.off("end", onEnd);
    stream.off("finish", onFinish);
    stream.off("close", onClose);
    stream.off("error", onError);
  };
  return cleanup;
}

// ---- compose -------------------------------------------------------------
function compose(a1: any = undefined, a2: any = undefined, a3: any = undefined, a4: any = undefined, a5: any = undefined, a6: any = undefined): any {
  const raw = [a1, a2, a3, a4, a5, a6];
  const streams: any[] = [];
  for (let i = 0; i < raw.length; i++) { if (raw[i] !== undefined) { streams.push(__toStream(raw[i])); } }
  const first = streams[0];
  const last = streams[streams.length - 1];
  for (let i = 0; i < streams.length - 1; i++) {
    if (typeof streams[i].pipe === "function") { streams[i].pipe(streams[i + 1]); }
  }
  const d = new Duplex({ objectMode: true });
  d._write = (chunk: any, enc: any, cb: any) => { first.write(chunk); cb(); };
  d._final = (cb: any) => { if (typeof first.end === "function") { first.end(); } cb(); };
  d._read = () => {};
  if (last !== undefined && typeof last.on === "function") {
    last.on("data", (chunk: any) => { __push(d, chunk, undefined, false); });
    last.on("end", () => { __push(d, null, undefined, false); });
    last.on("error", (e: any) => { d.destroy(e); });
    if (typeof last.resume === "function") { last.resume(); }
  }
  return d;
}

// ---- duplexPair ----------------------------------------------------------
class __PairDuplex extends Duplex {
  __other: any = null;
  constructor(options: any = undefined) { super(options); }
  _write(chunk: any, encoding: any, cb: any): void {
    if (this.__other !== null) { __push(this.__other, chunk, undefined, false); }
    cb();
  }
  _final(cb: any): void { if (this.__other !== null) { __push(this.__other, null, undefined, false); } cb(); }
  _read(size: any): void {}
}

function duplexPair(options: any = undefined): any {
  const a = new __PairDuplex(options);
  const b = new __PairDuplex(options);
  a.__other = b;
  b.__other = a;
  return [a, b];
}

// ---- predicates + abort + hwm --------------------------------------------
function isErrored(s: any): any { return s !== null && s !== undefined && s.errored !== null && s.errored !== undefined; }
function isReadable(s: any): any { return s !== null && s !== undefined && s.readable; }
function isWritable(s: any): any { return s !== null && s !== undefined && s.writable; }

function addAbortSignal(signal: any, stream: any): any {
  if (signal === null || signal === undefined) { return stream; }
  if (signal.aborted) { stream.destroy(__abortErr()); return stream; }
  if (typeof signal.addEventListener === "function") {
    signal.addEventListener("abort", () => { stream.destroy(__abortErr()); });
  }
  return stream;
}

// ==== stream/consumers ====================================================
function __consumeChunks(stream: any): any[] {
  if (stream !== null && stream !== undefined && typeof stream.getReader === "function") {
    const reader = stream.getReader();
    const out: any[] = [];
    let r = reader.read();
    while (r !== undefined && !r.done) { out.push(r.value); r = reader.read(); }
    return out;
  }
  if (stream !== null && stream !== undefined && typeof stream.next === "function") {
    const out: any[] = [];
    let n = stream.next();
    while (n !== undefined && !n.done) { out.push(n.value); n = stream.next(); }
    return out;
  }
  return __rDrain(stream);
}

function __consumerBuffer(stream: any): any {
  const chunks = __consumeChunks(stream);
  const bufs: any[] = [];
  for (let i = 0; i < chunks.length; i++) {
    const c = chunks[i];
    if (typeof c === "string") { bufs.push(Buffer.from(c)); }
    else { bufs.push(Buffer.from(c)); }
  }
  return Buffer.concat(bufs);
}

function __streamConsumersText(stream: any): any {
  const chunks = __consumeChunks(stream);
  let out = "";
  for (let i = 0; i < chunks.length; i++) {
    const c = chunks[i];
    out += typeof c === "string" ? c : __utf8_decode(c);
  }
  return Promise.resolve(out);
}
function __streamConsumersJson(stream: any): any {
  const chunks = __consumeChunks(stream);
  let out = "";
  for (let i = 0; i < chunks.length; i++) {
    const c = chunks[i];
    out += typeof c === "string" ? c : __utf8_decode(c);
  }
  return Promise.resolve(JSON.parse(out));
}
function __streamConsumersBuffer(stream: any): any { return Promise.resolve(__consumerBuffer(stream)); }
function __streamConsumersArrayBuffer(stream: any): any {
  const b = __consumerBuffer(stream);
  return Promise.resolve(b.buffer !== undefined ? b.buffer : b);
}
function __streamConsumersBytes(stream: any): any {
  const b = __consumerBuffer(stream);
  return Promise.resolve(new Uint8Array(b));
}
function __streamConsumersBlob(stream: any): any {
  const b = __consumerBuffer(stream);
  return Promise.resolve(new Blob([b]));
}

// ==== stream/promises =====================================================
function __streamPromisesPipeline(a1: any, a2: any = undefined, a3: any = undefined, a4: any = undefined, a5: any = undefined, a6: any = undefined): any {
  return new Promise((resolve: any, reject: any) => {
    const done = (err: any) => { if (err !== null && err !== undefined) { reject(err); } else { resolve(undefined); } };
    pipeline(a1, a2, a3, a4, a5, a6, done, undefined);
  });
}
function __streamPromisesFinished(stream: any, opts: any = undefined): any {
  return new Promise((resolve: any, reject: any) => {
    finished(stream, opts, (err: any) => { if (err !== null && err !== undefined) { reject(err); } else { resolve(undefined); } });
  });
}
