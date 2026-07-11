// node:fs — readFileSync honors the encoding.
import { describe, test, expect } from "rts:test";
import { mkdtempSync, writeFileSync, readFileSync, rmSync, existsSync } from "node:fs";
const d = mkdtempSync("__rts_readenc_");
const f = d + "/f.txt";
writeFileSync(f, "abc"); // bytes 61 62 63
const hex = readFileSync(f, "hex");
const b64 = readFileSync(f, "base64");
const utf8 = readFileSync(f, "utf8");
const hexOk = hex === "616263";
const b64Ok = b64 === "YWJj";
const utf8Ok = utf8 === "abc";
const opts = { recursive: true };
rmSync(d, opts);
const cleaned = existsSync(d) === false;
describe("node:fs readFileSync encoding", () => {
    test("hex", () => expect(hexOk).toBe(true));
    test("base64", () => expect(b64Ok).toBe(true));
    test("utf8", () => expect(utf8Ok).toBe(true));
    test("cleanup", () => expect(cleaned).toBe(true));
});
