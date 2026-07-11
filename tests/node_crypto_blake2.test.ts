// node:crypto — BLAKE2 support.
import { describe, test, expect } from "rts:test";
import { createHash, getHashes } from "node:crypto";

// BLAKE2b-512("abc") = ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d1
//   7d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923
const hb = createHash("blake2b512");
hb.update("abc");
const b2bOk = hb.digest("hex") === "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923";

// BLAKE2s-256("abc") = 508c5e8c327c14e2e1a72ba34eeb452f37458b209ed63a294d999b4c86675982
const hs = createHash("blake2s256");
hs.update("abc");
const b2sOk = hs.digest("hex") === "508c5e8c327c14e2e1a72ba34eeb452f37458b209ed63a294d999b4c86675982";

const listedOk = getHashes().indexOf("blake2b512") >= 0;

describe("node:crypto blake2", () => {
    test("blake2b512(abc)", () => expect(b2bOk).toBe(true));
    test("blake2s256(abc)", () => expect(b2sOk).toBe(true));
    test("getHashes lists blake2", () => expect(listedOk).toBe(true));
});
