// node:fs — mkdtempSync + readlinkSync.
import { describe, test, expect } from "rts:test";
import { mkdtempSync, existsSync, statSync, rmSync } from "node:fs";

const dir = mkdtempSync("__rts_tmp_");
const created = existsSync(dir) && statSync(dir).isDirectory();
const named = dir.indexOf("__rts_tmp_") === 0 && dir.length === "__rts_tmp_".length + 6;

// two calls yield different dirs.
const dir2 = mkdtempSync("__rts_tmp_");
const distinct = dir !== dir2;

const opts = { recursive: true };
rmSync(dir, opts);
rmSync(dir2, opts);
const cleaned = existsSync(dir) === false && existsSync(dir2) === false;

describe("node:fs mkdtempSync", () => {
    test("creates a directory", () => expect(created).toBe(true));
    test("prefix + 6 chars", () => expect(named).toBe(true));
    test("unique per call", () => expect(distinct).toBe(true));
    test("cleanup", () => expect(cleaned).toBe(true));
});
