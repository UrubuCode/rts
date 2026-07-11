// node:fs — cpSync (file + recursive directory copy).
import { describe, test, expect } from "rts:test";
import { mkdtempSync, writeFileSync, mkdirSync, readFileSync, existsSync, cpSync, rmSync } from "node:fs";

const root = mkdtempSync("__rts_cp_");
// build a tree: root/a.txt, root/sub/b.txt
writeFileSync(root + "/a.txt", "hello");
mkdirSync(root + "/sub");
writeFileSync(root + "/sub/b.txt", "world");

// single file copy.
cpSync(root + "/a.txt", root + "/a_copy.txt");
const fileCopyOk = existsSync(root + "/a_copy.txt") && readFileSync(root + "/a_copy.txt", "utf8") === "hello";

// recursive directory copy.
const opts = { recursive: true };
cpSync(root + "/sub", root + "/sub_copy", opts);
const dirCopyOk = existsSync(root + "/sub_copy/b.txt") && readFileSync(root + "/sub_copy/b.txt", "utf8") === "world";

const rmOpts = { recursive: true };
rmSync(root, rmOpts);
const cleaned = existsSync(root) === false;

describe("node:fs cpSync", () => {
    test("file copy", () => expect(fileCopyOk).toBe(true));
    test("recursive dir copy", () => expect(dirCopyOk).toBe(true));
    test("cleanup", () => expect(cleaned).toBe(true));
});
