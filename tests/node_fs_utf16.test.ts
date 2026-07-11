// node:fs — utf16le read/write round-trip.
import { describe, test, expect } from "rts:test";
import { mkdtempSync, writeFileSync, readFileSync, rmSync, existsSync } from "node:fs";
const d = mkdtempSync("__rts_fsu16_");
const f = d + "/f.txt";
writeFileSync(f, "hi", "utf16le");        // 4 bytes: h,0,i,0
const buf = readFileSync(f);              // raw bytes
const rawOk = buf.length === 4 && buf[0] === 104 && buf[1] === 0 && buf[2] === 105;
const back = readFileSync(f, "utf16le");  // decode back to "hi"
const roundOk = back === "hi";
const opts = { recursive: true };
rmSync(d, opts);
const cleaned = existsSync(d) === false;
describe("node:fs utf16le", () => {
    test("utf16le bytes on disk", () => expect(rawOk).toBe(true));
    test("utf16le round-trip", () => expect(roundOk).toBe(true));
    test("cleanup", () => expect(cleaned).toBe(true));
});
