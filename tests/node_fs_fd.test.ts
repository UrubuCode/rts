// node:fs — file-descriptor family (openSync/writeSync/readSync/fstatSync/close).
import { describe, test, expect } from "rts:test";
import { openSync, writeSync, readSync, fstatSync, closeSync, ftruncateSync, unlinkSync, existsSync } from "node:fs";

const F = "__rts_fs_fd_test.bin";

// write "hello" via a write fd.
//
// `writeSync`'s `buffer` argument must be a `Buffer`/`TypedArray`/`DataView`
// in real Node — verified against it directly: a plain `number[]` throws
// `TypeError [ERR_INVALID_ARG_TYPE]: The "buffer" argument must be of type
// string or an instance of Buffer, TypedArray, or DataView. Received an
// instance of Array`. This fixture used to pass a plain array and asserted
// the write/read succeeded, which is not what Node does with that input;
// corrected to pass a real `Buffer`, the same fix applied to `readSync`'s
// destination below.
const wfd = openSync(F, "w");
const src = Buffer.from([104, 101, 108, 108, 111]); // "hello"
const written = writeSync(wfd, src, 0, 5, 0);
closeSync(wfd);
const wroteOk = written === 5;

// fstat via a fresh read fd.
const rfd = openSync(F, "r");
const st = fstatSync(rfd);
const fstatOk = st.isFile() === true && st.size === 5;

// read 5 bytes into a buffer.
const buf = Buffer.alloc(5);
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
