// node:crypto — RIPEMD-160.
import { describe, test, expect } from "rts:test";
import { createHash, getHashes } from "node:crypto";
// RIPEMD-160("abc") = 8eb208f7e05d987a9b044a8e98c6b087f15a0bfc
const h = createHash("ripemd160");
h.update("abc");
const ok = h.digest("hex") === "8eb208f7e05d987a9b044a8e98c6b087f15a0bfc";
const listed = getHashes().indexOf("ripemd160") >= 0;
describe("node:crypto ripemd160", () => {
    test("ripemd160(abc)", () => expect(ok).toBe(true));
    test("getHashes lists ripemd160", () => expect(listed).toBe(true));
});
