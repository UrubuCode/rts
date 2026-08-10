import { describe, test, expect } from "rts:test";
import {
  Readable, Writable, Duplex, Transform, PassThrough,
  duplexPair, isWritable,
  getDefaultHighWaterMark,
} from "node:stream";

// ---- Readable push/read/flow ----------------------------------------------
const readOut: any[] = [];
let end1 = false;
const r1: any = new Readable({ objectMode: true, read: () => {} });
r1.on("data", (c: any) => { readOut.push(c); });
r1.on("end", () => { end1 = true; });
r1.push("a");
r1.push("b");
r1.push(null);

// ---- Readable paused read() -----------------------------------------------
const r2: any = new Readable({ read: () => {} });
r2.push("hello");
const readChunk = r2.read();

// ---- Writable accumulate --------------------------------------------------
const written: any[] = [];
let finish1 = false;
const w1: any = new Writable({
  objectMode: true,
  write: (chunk: any, enc: any, cb: any) => { written.push(chunk); cb(); },
});
w1.on("finish", () => { finish1 = true; });
w1.write("x");
w1.write("y");
w1.end("z");

// ---- Transform uppercase --------------------------------------------------
const upperOut: any[] = [];
const t1: any = new Transform({
  transform: (chunk: any, enc: any, cb: any) => { cb(null, ("" + chunk).toUpperCase()); },
});
t1.on("data", (c: any) => { upperOut.push(c); });
t1.write("ab");
t1.write("cd");
t1.end();

// ---- PassThrough ----------------------------------------------------------
const ptOut: any[] = [];
const pt: any = new PassThrough({ objectMode: true });
pt.on("data", (c: any) => { ptOut.push(c); });
pt.write(1);
pt.write(2);
pt.end();

// ---- pipe -----------------------------------------------------------------
const pipeDst: any[] = [];
const src: any = new Readable({ objectMode: true, read: () => {} });
src.push(10); src.push(20); src.push(30); src.push(null);
const dst: any = new Writable({
  objectMode: true,
  write: (chunk: any, enc: any, cb: any) => { pipeDst.push(chunk); cb(); },
});
let pipeDone = false;
dst.on("finish", () => { pipeDone = true; });
src.pipe(dst);

// ---- Duplex echo ----------------------------------------------------------
const duplexOut: any[] = [];
const d1: any = new Duplex({
  objectMode: true,
  read: () => {},
  write: (chunk: any, enc: any, cb: any) => { d1.push(chunk); cb(); },
});
d1.on("data", (c: any) => { duplexOut.push(c); });
d1.write("echo");
d1.resume();

// ---- duplexPair -----------------------------------------------------------
const pair: any = duplexPair();
const pairA: any = pair[0];
const pairB: any = pair[1];
const pairRecv: any[] = [];
pairB.on("data", (c: any) => { pairRecv.push(c); });
pairB.resume();
pairA.write("ping");
pairA.end();

// ---- backpressure ---------------------------------------------------------
// A stalled sink (cb never fired) so the buffer fills past highWaterMark and
// write() returns false — the backpressure signal.
const bpW: any = new Writable({
  highWaterMark: 2, objectMode: true,
  write: (chunk: any, enc: any, cb: any) => { /* stall: never call cb */ },
});
bpW.write(1);
const bpOk = bpW.write(2);

// ---- once('data') also promotes to flowing ---------------------------------
// It has to be overridden separately from on(): EventEmitter.once appends to
// the listener table directly instead of routing through the overridden on().
const onceOut: any[] = [];
const r3: any = new Readable({ objectMode: true, read: () => {} });
r3.once("data", (c: any) => { onceOut.push(c); });
r3.push("only");
r3.push("second");
r3.push(null);

// ---- .on('data').on('end') chained -----------------------------------------
// The pattern deferred 'end' delivery exists for: the 'end' listener is
// attached AFTER the flow has already started and drained, and must still run.
// Emitting 'end' inside the .on('data', …) call skips it entirely.
let chainedEnd = false;
const chainedOut: any[] = [];
const r4: any = new Readable({ objectMode: true, read: () => {} });
r4.push("x");
r4.push(null);
r4.on("data", (c: any) => { chainedOut.push(c); }).on("end", () => { chainedEnd = true; });

// ---- for await over a Readable ---------------------------------------------
// Buffered chunks answer already-fulfilled promises, so this whole iteration
// finishes without a loop turn.
const iterOut: any[] = [];
let iterDone = false;
const r5: any = new Readable({ objectMode: true, read: () => {} });
r5.push(1); r5.push(2); r5.push(3); r5.push(null);
async function drainBuffered() {
  for await (const c of r5) { iterOut.push(c); }
  iterDone = true;
}
drainBuffered();

// ---- for await over a chunk that has not arrived yet -----------------------
// The parked path: next() answers a pending promise, and the push from a timer
// callback settles it. The timer is what keeps the loop turning.
const lateOut: any[] = [];
let lateDone = false;
const r6: any = new Readable({ objectMode: true, read: () => {} });
setTimeout(() => { r6.push("late"); r6.push(null); }, 1);
async function drainLate() {
  for await (const c of r6) { lateOut.push(c); }
  lateDone = true;
}
drainLate();

// `break` out of a `for await` over a Readable is pinned by
// `tests/for_await_break_return.test.ts` instead of here, and the reason is
// which layer owns the defect: this engine's emitter does not call the
// iterator's `return()` on `break` AT ALL — measured over a plain object with
// a `[Symbol.asyncIterator]`, no stream in sight. Asserting it here would turn
// a green `node:stream` file red for something no change to `node:stream`
// could fix.

// Every assertion below runs one loop turn later, because that is when Node
// delivers 'end' too — see `crates/rts-node/src/stream/flowing.rs`. It used
// to assert `end1`/`pipeDone` synchronously, which passed only because this
// engine emitted 'end' inside the call that started the flow; real Node fails
// those same two lines. `suite_run` reads the record after the host's loop has
// drained, so a test registered from here is still counted.
setTimeout(() => {
describe("node:stream", () => {
  test("Readable push/read/flow", () => {
    expect(readOut.length).toBe(2);
    expect(readOut[0]).toBe("a");
    expect(readOut[1]).toBe("b");
    expect(end1).toBe(true);
    expect(r1.readableEnded).toBe(true);
  });
  test("Readable read() paused", () => {
    expect(readChunk).toBe("hello");
  });
  test("Writable finish + order", () => {
    expect(written.length).toBe(3);
    expect(written[2]).toBe("z");
    expect(finish1).toBe(true);
    expect(w1.writableFinished).toBe(true);
  });
  test("Transform uppercase", () => {
    expect(upperOut.length).toBe(2);
    expect(upperOut[0]).toBe("AB");
    expect(upperOut[1]).toBe("CD");
  });
  test("PassThrough", () => {
    expect(ptOut.length).toBe(2);
    expect(ptOut[0]).toBe(1);
  });
  test("pipe end-to-end", () => {
    expect(pipeDst.length).toBe(3);
    expect(pipeDst[0]).toBe(10);
    expect(pipeDone).toBe(true);
  });
  test("Duplex echo", () => {
    expect(duplexOut.length).toBe(1);
    expect(duplexOut[0]).toBe("echo");
  });
  test("duplexPair cross delivery", () => {
    expect(pairRecv.length).toBe(1);
    expect(pairRecv[0]).toBe("ping");
  });
  test("backpressure signal", () => {
    expect(bpOk).toBe(false);
  });
  test("predicates + hwm", () => {
    expect(getDefaultHighWaterMark(true)).toBe(16);
    expect(getDefaultHighWaterMark(false)).toBe(16384);
    expect(isWritable(new PassThrough())).toBe(true);
  });
  test("once('data') promotes to flowing", () => {
    expect(onceOut.length).toBe(1);
    expect(onceOut[0]).toBe("only");
  });
  test("'end' reaches a listener attached after the flow started", () => {
    expect(chainedOut.length).toBe(1);
    expect(chainedOut[0]).toBe("x");
    expect(chainedEnd).toBe(true);
    expect(r4.readableEnded).toBe(true);
  });
  test("for await over buffered chunks", () => {
    expect(iterDone).toBe(true);
    expect(iterOut.length).toBe(3);
    expect(iterOut[0]).toBe(1);
    expect(iterOut[2]).toBe(3);
  });
  test("for await waits for a chunk pushed later", () => {
    expect(lateDone).toBe(true);
    expect(lateOut.length).toBe(1);
    expect(lateOut[0]).toBe("late");
  });
});
}, 5);
