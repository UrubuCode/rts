// node:fs — writeFileSync honors the encoding for string data.
import { describe, test, expect } from "rts:test";
import { mkdtempSync, writeFileSync, readFileSync, rmSync, existsSync } from "node:fs";
const d = mkdtempSync("__rts_writeenc_");
const f = d + "/f.bin";
// write hex "616263" → the 3 bytes a,b,c; read back as utf8 → "abc".
writeFileSync(f, "616263", "hex");
const asUtf8 = readFileSync(f, "utf8");
const hexOk = asUtf8 === "abc";
// base64 "YWJj" → "abc".
writeFileSync(f, "YWJj", "base64");
const b64Ok = readFileSync(f, "utf8") === "abc";
const opts = { recursive: true };
rmSync(d, opts);
const cleaned = existsSync(d) === false;
describe("node:fs writeFileSync encoding", () => {
    test("hex data", () => expect(hexOk).toBe(true));
    test("base64 data", () => expect(b64Ok).toBe(true));
    test("cleanup", () => expect(cleaned).toBe(true));
});
