// node:fs — synchronous filesystem surface. Operates on temp paths in the cwd,
// cleaning up after itself.
import { describe, test, expect } from "rts:test";
import {
    writeFileSync,
    readFileSync,
    appendFileSync,
    existsSync,
    accessSync,
    mkdirSync,
    rmdirSync,
    rmSync,
    unlinkSync,
    renameSync,
    copyFileSync,
    truncateSync,
    readdirSync,
    statSync,
    lstatSync,
} from "node:fs";

const F = "__rts_fs_test.txt";
const F2 = "__rts_fs_test_renamed.txt";
const CP = "__rts_fs_test_copy.txt";
const D = "__rts_fs_test_dir";
const NEST = "__rts_fs_test_nest/a/b";

// --- write / read / exists --------------------------------------------------
writeFileSync(F, "hello");
const existsAfterWrite = existsSync(F);
const content = readFileSync(F, "utf8");
const contentOk = content === "hello";

// readFileSync without encoding → Buffer (Uint8Array), byte 0 = 'h' = 104.
const buf = readFileSync(F);
const bufOk = buf.length === 5 && buf[0] === 104;

// --- append -----------------------------------------------------------------
appendFileSync(F, " world");
const appended = readFileSync(F, "utf8") === "hello world";

// --- stat -------------------------------------------------------------------
const st = statSync(F);
const statOk = st.isFile() === true && st.isDirectory() === false && st.size === 11;
const lst = lstatSync(F);
const lstatOk = lst.isFile() === true;

// --- truncate ---------------------------------------------------------------
truncateSync(F, 5);
const truncOk = readFileSync(F, "utf8") === "hello" && statSync(F).size === 5;

// --- copy / rename ----------------------------------------------------------
copyFileSync(F, CP);
const copyOk = existsSync(CP) && readFileSync(CP, "utf8") === "hello";
renameSync(CP, F2);
const renameOk = existsSync(F2) === true && existsSync(CP) === false;

// --- mkdir / readdir / rmdir ------------------------------------------------
mkdirSync(D);
const dirExists = existsSync(D) && statSync(D).isDirectory();
writeFileSync(D + "/inner.txt", "x");
const listed = readdirSync(D);
const readdirOk = listed.length === 1 && listed[0] === "inner.txt";

// recursive mkdir + recursive rm (inline options object).
mkdirSync(NEST, { recursive: true });
const nestOk = existsSync(NEST);

// --- access (existing file → no throw) --------------------------------------
accessSync(F);
const accessOk = true; // reached here → accessSync did not throw for an existing file

// --- cleanup ----------------------------------------------------------------
unlinkSync(F);
unlinkSync(F2);
unlinkSync(D + "/inner.txt");
rmdirSync(D);
rmSync("__rts_fs_test_nest", { recursive: true });
const cleanedUp = existsSync(F) === false && existsSync(D) === false && existsSync("__rts_fs_test_nest") === false;

describe("node:fs", () => {
    test("writeFileSync + existsSync", () => expect(existsAfterWrite).toBe(true));
    test("readFileSync encoding", () => expect(contentOk).toBe(true));
    test("readFileSync Buffer", () => expect(bufOk).toBe(true));
    test("appendFileSync", () => expect(appended).toBe(true));
    test("statSync isFile/size", () => expect(statOk).toBe(true));
    test("lstatSync", () => expect(lstatOk).toBe(true));
    test("truncateSync", () => expect(truncOk).toBe(true));
    test("copyFileSync", () => expect(copyOk).toBe(true));
    test("renameSync", () => expect(renameOk).toBe(true));
    test("mkdirSync + isDirectory", () => expect(dirExists).toBe(true));
    test("readdirSync", () => expect(readdirOk).toBe(true));
    test("mkdirSync recursive", () => expect(nestOk).toBe(true));
    test("accessSync on existing file", () => expect(accessOk).toBe(true));
    test("cleanup", () => expect(cleanedUp).toBe(true));
});
