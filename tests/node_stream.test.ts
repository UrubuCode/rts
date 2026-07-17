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
});
