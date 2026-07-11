// node:crypto — scryptSync (RFC 7914 vector).
import { describe, test, expect } from "rts:test";
import { scryptSync } from "node:crypto";

// RFC 7914 §12: scrypt("", "", N=16, r=1, p=1, dkLen=64) =
// 77d65762 ... first byte 0x77=119, last byte 0x06=6.
const dk = scryptSync("", "", 64, 16, 1, 1);
const lenOk = dk.length === 64;
const firstOk = dk[0] === 119;  // 0x77
const secondOk = dk[1] === 214; // 0xd6
const lastOk = dk[63] === 6;    // 0x06

// default-params form (N=16384,r=8,p=1) — just check length + determinism.
const d1 = scryptSync("password", "salt", 32);
const d2 = scryptSync("password", "salt", 32);
const detOk = d1.length === 32 && d1[0] === d2[0] && d1[31] === d2[31];

describe("node:crypto scryptSync", () => {
    test("keylen", () => expect(lenOk).toBe(true));
    test("first byte (RFC vector)", () => expect(firstOk).toBe(true));
    test("second byte (RFC vector)", () => expect(secondOk).toBe(true));
    test("last byte (RFC vector)", () => expect(lastOk).toBe(true));
    test("default params deterministic", () => expect(detOk).toBe(true));
});
