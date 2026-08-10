// node:crypto — scryptSync (RFC 7914 vector).
import { describe, test, expect } from "rts:test";
import { scryptSync } from "node:crypto";

// RFC 7914 §12: scrypt("", "", N=16, r=1, p=1, dkLen=64) =
// 77d65762 ... first byte 0x77=119, last byte 0x06=6.
//
// Node's real signature is scryptSync(password, salt, keylen[, options]) —
// N/r/p are read from an OPTIONS OBJECT, the fourth argument, never from
// extra positional arguments. `scryptSync("", "", 64, 16, 1, 1)` passes a
// *number* (16) as that fourth argument, which Node's own validation accepts
// as "no options" (it only rejects non-nullish non-objects there for a few
// paths) and falls back to the defaults (N=16384, r=8, p=1) — verified
// against a real Node: it does NOT throw and does NOT match the RFC vector.
// The earlier form of this fixture asserted the RFC-vector bytes against
// that positional call, which is not what Node computes; corrected to pass
// the options object Node actually reads.
const dk = scryptSync("", "", 64, { N: 16, r: 1, p: 1 });
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
