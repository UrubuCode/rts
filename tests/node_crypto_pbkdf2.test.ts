// node:crypto — pbkdf2Sync (RFC-style known vector).
import { describe, test, expect } from "rts:test";
import { pbkdf2Sync } from "node:crypto";

// PBKDF2-HMAC-SHA256("password", "salt", 1, 32) =
// 120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b
const dk = pbkdf2Sync("password", "salt", 1, 32, "sha256");
const lenOk = dk.length === 32;
const firstOk = dk[0] === 18;   // 0x12
const lastOk = dk[31] === 123;  // 0x7b

// c=2 differs from c=1.
const dk2 = pbkdf2Sync("password", "salt", 2, 32, "sha256");
const iterDiffers = dk2[0] !== 18 || dk2[31] !== 123;

describe("node:crypto pbkdf2Sync", () => {
    test("keylen", () => expect(lenOk).toBe(true));
    test("first byte (sha256 c=1)", () => expect(firstOk).toBe(true));
    test("last byte (sha256 c=1)", () => expect(lastOk).toBe(true));
    test("iterations change output", () => expect(iterDiffers).toBe(true));
});
