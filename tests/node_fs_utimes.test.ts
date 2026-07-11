// node:fs — utimesSync sets modify time (reflected in statSync.mtimeMs).
import { describe, test, expect } from "rts:test";
import { mkdtempSync, writeFileSync, statSync, utimesSync, existsSync, rmSync } from "node:fs";

const d = mkdtempSync("__rts_utimes_");
const f = d + "/t.txt";
writeFileSync(f, "x");

// set mtime to 1_000_000 s (2001-ish); statSync.mtimeMs ~ 1_000_000_000 ms.
utimesSync(f, 1000000, 1000000);
const mtimeMs = statSync(f).mtimeMs;
const mtimeOk = mtimeMs > 999999000 && mtimeMs < 1000001000;

const rmOpts = { recursive: true };
rmSync(d, rmOpts);
const cleaned = existsSync(d) === false;

describe("node:fs utimesSync", () => {
    test("sets mtime", () => expect(mtimeOk).toBe(true));
    test("cleanup", () => expect(cleaned).toBe(true));
});
