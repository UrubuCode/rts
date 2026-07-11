// node:crypto — hkdfSync (RFC 5869 Test Case 1).
import { describe, test, expect } from "rts:test";
import { hkdfSync } from "node:crypto";

// IKM = 22 bytes of 0x0b; salt = 0x000102...0c (13 bytes); info = 0xf0..f9 (10);
// L = 42, SHA-256. Expected OKM starts 3cb25f25... ends ...3436587 (byte41=0x65).
const ikm: number[] = [];
let i = 0; while (i < 22) { ikm.push(11); i = i + 1; }
const salt: number[] = [];
i = 0; while (i < 13) { salt.push(i); i = i + 1; }
const info: number[] = [];
i = 0; while (i < 10) { info.push(240 + i); i = i + 1; }

const okm = hkdfSync("sha256", ikm, salt, info, 42);
const lenOk = okm.length === 42;
const firstOk = okm[0] === 60;  // 0x3c
const secondOk = okm[1] === 178; // 0xb2
const lastOk = okm[41] === 101; // 0x65

describe("node:crypto hkdfSync", () => {
    test("keylen", () => expect(lenOk).toBe(true));
    test("first byte (RFC 5869)", () => expect(firstOk).toBe(true));
    test("second byte (RFC 5869)", () => expect(secondOk).toBe(true));
    test("last byte (RFC 5869)", () => expect(lastOk).toBe(true));
});
