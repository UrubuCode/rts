// Web Streams — ReadableStream / WritableStream / TransformStream /
// TextEncoderStream / TextDecoderStream — rts-shared stdlib prelude (NOT
// primordial; no native syntax). Pure `.ts` state machines over plain arrays:
// a ReadableStream's controller queues chunks (or FORWARDS them downstream
// once `pipeThrough` wires a destination); readers drain synchronously —
// `await reader.read()` rides the non-Promise `await` passthrough. Underlying
// sources/sinks/transformers are duck-typed (`'start' in source`), and every
// internal helper is a REAL class (never an object literal inside a
// constructor — the literal-method recovery does not reach ctor bodies yet).

class RSDController {
  __q: any[] = [];
  __closed: boolean = false;
  // Once piped, chunks FORWARD to the downstream sink instead of queueing.
  __fwd: any = null;
  enqueue(chunk: any): void {
    const f = this.__fwd;
    if (f !== null) { f.write(chunk); return; }
    this.__q.push(chunk);
  }
  close(): void {
    this.__closed = true;
    const f = this.__fwd;
    if (f !== null) { f.close(); }
  }
  error(e: any): void { this.__closed = true; }
  get desiredSize(): number { return 1; }
}

class RSDReader {
  __s: any = null;
  read(): any {
    const ctl = this.__s.__ctl;
    const q = ctl.__q;
    if (q.length > 0) {
      const v = q[0];
      ctl.__q = q.slice(1);
      return { done: false, value: v };
    }
    return { done: true, value: undefined };
  }
  cancel(reason: any = undefined): any { return undefined; }
  releaseLock(): void {}
}

class ReadableStream {
  __ctl: RSDController;
  locked: boolean = false;
  constructor(source: any = undefined) {
    this.__ctl = new RSDController();
    if (source !== undefined && source !== null) {
      if ("start" in source) { source.start(this.__ctl); }
    }
  }
  getReader(): RSDReader {
    const r = new RSDReader();
    r.__s = this;
    return r;
  }
  // Drain what is already queued into the pair's writable, then FORWARD every
  // later enqueue (lazy piping — the common `pipeThrough` before the writes).
  pipeThrough(pair: any): any {
    const sink = pair.writable.__sink;
    const ctl = this.__ctl;
    const q = ctl.__q;
    for (let i = 0; i < q.length; i++) { sink.write(q[i]); }
    ctl.__q = [];
    ctl.__fwd = sink;
    if (ctl.__closed) { sink.close(); }
    return pair.readable;
  }
}

class WSDWriter {
  __s: any = null;
  write(chunk: any): any {
    this.__s.__sink.write(chunk);
    return undefined;
  }
  close(): any {
    this.__s.__sink.close();
    return undefined;
  }
  abort(reason: any = undefined): any { return undefined; }
  releaseLock(): void {}
}

// The ONE internal sink: `__mode` selects the per-chunk behavior; `__ctl` is
// the destination ReadableStream controller.
class __StreamSink {
  __t: any = null;
  __ctl: any = null;
  __mode: string = "identity";
  write(chunk: any): void {
    const c = this.__ctl;
    if (this.__mode === "transform") {
      const t = this.__t;
      if (t !== undefined && t !== null && "transform" in t) {
        t.transform(chunk, c);
        return;
      }
      c.enqueue(chunk);
      return;
    }
    if (this.__mode === "encode") { c.enqueue(__utf8_encode("" + chunk)); return; }
    if (this.__mode === "decode") { c.enqueue(__utf8_decode(chunk)); return; }
    c.enqueue(chunk);
  }
  close(): void {
    if (this.__mode === "transform") {
      const t = this.__t;
      if (t !== undefined && t !== null && "flush" in t) { t.flush(this.__ctl); }
    }
    this.__ctl.close();
  }
}

class WritableStream {
  __sink: __StreamSink;
  locked: boolean = false;
  constructor(sink: any = undefined) {
    const s = new __StreamSink();
    s.__ctl = new RSDController();
    if (sink !== undefined && sink !== null) {
      s.__t = sink;
      s.__mode = "transform";
    }
    this.__sink = s;
  }
  getWriter(): WSDWriter {
    const w = new WSDWriter();
    w.__s = this;
    return w;
  }
}

class TransformStream {
  readable: ReadableStream;
  writable: WritableStream;
  constructor(transformer: any = undefined) {
    const rs = new ReadableStream();
    const ws = new WritableStream();
    const sink = ws.__sink;
    sink.__ctl = rs.__ctl;
    sink.__mode = "transform";
    if (transformer !== undefined && transformer !== null) { sink.__t = transformer; }
    else { sink.__t = null; }
    this.readable = rs;
    this.writable = ws;
  }
}

class TextEncoderStream {
  readable: ReadableStream;
  writable: WritableStream;
  constructor() {
    const rs = new ReadableStream();
    const ws = new WritableStream();
    const sink = ws.__sink;
    sink.__ctl = rs.__ctl;
    sink.__mode = "encode";
    this.readable = rs;
    this.writable = ws;
  }
  get encoding(): string { return "utf-8"; }
}

class TextDecoderStream {
  readable: ReadableStream;
  writable: WritableStream;
  constructor(label: any = undefined) {
    const rs = new ReadableStream();
    const ws = new WritableStream();
    const sink = ws.__sink;
    sink.__ctl = rs.__ctl;
    sink.__mode = "decode";
    this.readable = rs;
    this.writable = ws;
  }
  get encoding(): string { return "utf-8"; }
}
