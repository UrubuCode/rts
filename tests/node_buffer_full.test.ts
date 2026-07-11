// node:buffer — Buffer statics + atob/btoa.
import { describe, test, expect } from "rts:test";
import { atob, btoa } from "node:buffer";

// base64 globals.
const b64 = btoa("hello");        // aGVsbG8=
const back = atob("aGVsbG8=");    // hello
const b64Ok = b64 === "aGVsbG8=";
const backOk = back === "hello";

// Buffer statics (ambient class).
const buf = Buffer.from("abc");
const fromOk = buf.length === 3 && buf[0] === 97 && buf[2] === 99;

const z = Buffer.alloc(4);
const allocOk = z.length === 4 && z[0] === 0 && z[3] === 0;

const hexBuf = Buffer.from("616263", "hex");
const hexOk = hexBuf.length === 3 && hexBuf[0] === 97;

const bl = Buffer.byteLength("héllo"); // é is 2 bytes utf8 → 6
const blOk = bl === 6;

const isB = Buffer.isBuffer(buf);
const notB = Buffer.isBuffer("string");
const cmp = Buffer.compare(Buffer.from("a"), Buffer.from("b")); // -1

describe("node:buffer", () => {
    test("btoa", () => expect(b64Ok).toBe(true));
    test("atob", () => expect(backOk).toBe(true));
    test("Buffer.from utf8", () => expect(fromOk).toBe(true));
    test("Buffer.alloc zeroed", () => expect(allocOk).toBe(true));
    test("Buffer.from hex", () => expect(hexOk).toBe(true));
    test("Buffer.byteLength utf8", () => expect(blOk).toBe(true));
    test("Buffer.isBuffer true", () => expect(isB).toBe(true));
    test("Buffer.isBuffer false", () => expect(notB).toBe(false));
    test("Buffer.compare", () => expect(cmp).toBe(-1));
});
