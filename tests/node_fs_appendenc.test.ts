// node:fs — appendFileSync honors the encoding.
import { describe, test, expect } from "rts:test";
import { mkdtempSync, writeFileSync, appendFileSync, readFileSync, rmSync, existsSync } from "node:fs";
const d = mkdtempSync("__rts_appendenc_");
const f = d + "/f.bin";
writeFileSync(f, "61", "hex");          // "a"
appendFileSync(f, "6263", "hex");       // + "bc" → "abc"
const ok = readFileSync(f, "utf8") === "abc";
const opts = { recursive: true };
rmSync(d, opts);
const cleaned = existsSync(d) === false;
describe("node:fs appendFileSync encoding", () => {
    test("hex append", () => expect(ok).toBe(true));
    test("cleanup", () => expect(cleaned).toBe(true));
});
