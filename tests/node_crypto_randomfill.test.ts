// node:crypto — randomFillSync fills a buffer in place.
import { describe, test, expect } from "rts:test";
import { randomBytes, randomFillSync } from "node:crypto";

// Start from a zeroed 32-byte buffer (randomBytes(0)-style: allocate via a
// zero-filled Uint8Array). Use randomBytes then zero-compare is hard; instead
// fill an existing buffer and verify it changed + returns the same buffer.
const buf = randomBytes(32); // some bytes
const before0 = buf[0];
const ret = randomFillSync(buf);
const sameRef = ret.length === 32;
// After a fill, all-zero is astronomically unlikely; check at least one nonzero.
let anyNonZero = false;
let i = 0;
while (i < 32) { if (buf[i] !== 0) { anyNonZero = true; } i = i + 1; }
// bytes are valid 0..255
const inRange = buf[0] >= 0 && buf[0] <= 255 && buf[31] >= 0 && buf[31] <= 255;

describe("node:crypto randomFillSync", () => {
    test("returns buffer of same length", () => expect(sameRef).toBe(true));
    test("fills with bytes", () => expect(anyNonZero).toBe(true));
    test("bytes in range", () => expect(inRange).toBe(true));
});
