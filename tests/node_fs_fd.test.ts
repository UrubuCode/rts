// node:fs — file-descriptor family (openSync/writeSync/readSync/fstatSync/close).
import { describe, test, expect } from "rts:test";
import { openSync, writeSync, readSync, fstatSync, closeSync, ftruncateSync, unlinkSync, existsSync } from "node:fs";

const F = "__rts_fs_fd_test.bin";

// write "hello" via a write fd.
const wfd = openSync(F, "w");
const src = [104, 101, 108, 108, 111]; // "hello"
const written = writeSync(wfd, src, 0, 5, 0);
closeSync(wfd);
const wroteOk = written === 5;

// fstat via a fresh read fd.
const rfd = openSync(F, "r");
const st = fstatSync(rfd);
const fstatOk = st.isFile() === true && st.size === 5;

// read 5 bytes into a buffer.
const buf = [0, 0, 0, 0, 0];
const nread = readSync(rfd, buf, 0, 5, 0);
const readOk = nread === 5 && buf[0] === 104 && buf[4] === 111;
closeSync(rfd);

// ftruncate to 3.
const tfd = openSync(F, "r+");
ftruncateSync(tfd, 3);
closeSync(tfd);
const truncOk = fstatSizeOf(F) === 3;

function fstatSizeOf(path: string): number {
    const fd = openSync(path, "r");
    const s = fstatSync(fd);
    closeSync(fd);
    return s.size;
}

unlinkSync(F);
const cleaned = existsSync(F) === false;

describe("node:fs fd family", () => {
    test("writeSync", () => expect(wroteOk).toBe(true));
    test("fstatSync", () => expect(fstatOk).toBe(true));
    test("readSync", () => expect(readOk).toBe(true));
    test("ftruncateSync", () => expect(truncOk).toBe(true));
    test("cleanup", () => expect(cleaned).toBe(true));
});
