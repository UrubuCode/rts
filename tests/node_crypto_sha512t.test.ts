// node:crypto — SHA-512/256 and SHA-512/224 (NIST vectors).
import { describe, test, expect } from "rts:test";
import { createHash } from "node:crypto";
// SHA-512/256("abc") = 53048e2681941ef99b2e29b76b4c7dabe4c2d0c634fc6d46e0e2f13107e7af23
const h256 = createHash("sha512-256");
h256.update("abc");
const s256Ok = h256.digest("hex") === "53048e2681941ef99b2e29b76b4c7dabe4c2d0c634fc6d46e0e2f13107e7af23";
// SHA-512/224("abc") = 4634270f707b6a54daae7530460842e20e37ed265ceee9a43e8924aa
const h224 = createHash("sha512-224");
h224.update("abc");
const s224Ok = h224.digest("hex") === "4634270f707b6a54daae7530460842e20e37ed265ceee9a43e8924aa";
describe("node:crypto sha512-t", () => {
    test("sha512-256(abc)", () => expect(s256Ok).toBe(true));
    test("sha512-224(abc)", () => expect(s224Ok).toBe(true));
});
