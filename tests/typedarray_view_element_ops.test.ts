// Level-B typed-array element read/write (a view over an ArrayBuffer). The
// runtime path was rewritten to read the view's fixed slots positionally and to
// take the index straight off the key PolyValue instead of stringify+reparse
// (CRANELIFT_IMPLEMENTATION.md step 5). These tests pin the SEMANTICS that
// rewrite must preserve — element wrap, cross-view sharing, byte widths, and
// out-of-range behaviour — not the speed.
import { describe, test, expect } from "rts:test";

// Two views over ONE buffer must see each other's writes (shared bytes).
const buf = new ArrayBuffer(8);
const u8 = new Uint8Array(buf);
const i8 = new Int8Array(buf);
u8[0] = 200;
const sharedI8 = i8[0]; // 200 as a signed byte = -56
u8[1] = 5;
const sharedU8 = u8[1];

// Element WRAP on write (ToUintN / ToIntN), not truncation.
const wrapU8 = new Uint8Array(new ArrayBuffer(4));
wrapU8[0] = 256; // wraps to 0
wrapU8[1] = 257; // wraps to 1
wrapU8[2] = -1; // wraps to 255
const w0 = wrapU8[0];
const w1 = wrapU8[1];
const w2 = wrapU8[2];

// A 32-bit view: byte width and little-endian decode through the shared buffer.
// NB: the two views are built over the SAME `ab` directly. Aliasing through the
// `.buffer` accessor (`new Uint8Array(u32.buffer)`) is a SEPARATE pre-existing
// bug — `.buffer` does not return the shared ArrayBuffer handle — and is out of
// scope for the element read/write path this file covers.
const ab32 = new ArrayBuffer(8);
const u32 = new Uint32Array(ab32);
u32[0] = 0x01020304;
const back = u32[0];
const bytes = new Uint8Array(ab32);
const lowByte = bytes[0]; // little-endian → 0x04

// Out-of-range read is undefined; OOB write is a silent no-op.
const small = new Uint8Array(new ArrayBuffer(2));
const oob = small[5];
small[9] = 1; // no-op, must not crash
const stillZero = small[0];

// Reading back a value written in a loop (the hot path this change touched).
const loop = new Uint8Array(new ArrayBuffer(16));
let s = 0;
for (let i = 0; i < 16; i = i + 1) loop[i] = i * 3;
for (let i = 0; i < 16; i = i + 1) s = s + loop[i];

// Two views over DIFFERENT buffers, interleaved in one loop. The step-5b
// hoisting caches (base, count) per view LOCAL; a per-name cache must never
// serve view `mixA`'s base for `mixB`. Different element widths too (u8 vs u16).
const mixA = new Uint8Array(new ArrayBuffer(4));
const mixB = new Uint16Array(new ArrayBuffer(8));
mixA[0] = 10;
mixA[1] = 20;
mixB[0] = 1000;
mixB[1] = 2000;
let mixSum = 0;
for (let i = 0; i < 2; i = i + 1) mixSum = mixSum + mixA[i] + mixB[i];

describe("level-B typed array element ops", () => {
  test("two views over one buffer share bytes", () => {
    expect(sharedI8).toBe(-56);
    expect(sharedU8).toBe(5);
  });

  test("writes wrap into the element domain", () => {
    expect(w0).toBe(0);
    expect(w1).toBe(1);
    expect(w2).toBe(255);
  });

  test("32-bit width + little-endian decode", () => {
    expect(back).toBe(0x01020304);
    expect(lowByte).toBe(0x04);
  });

  test("out-of-range read is undefined, OOB write is a no-op", () => {
    expect(oob).toBe(undefined);
    expect(stillZero).toBe(0);
  });

  test("loop write then read sums correctly", () => {
    // sum of 0,3,6,...,45 = 3 * (0+1+...+15) = 3 * 120 = 360
    expect(s).toBe(360);
  });

  test("two interleaved views keep independent hoisted bases", () => {
    // 10 + 20 + 1000 + 2000
    expect(mixSum).toBe(3030);
  });
});
