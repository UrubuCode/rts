// Streaming SHA-256 against the official vectors, on the `node:crypto` object
// surface.
//
// The file used to check the low-level `rts.crypto` handle surface against
// `node:crypto`, digest for digest. `rts.crypto` is going away, so both halves
// now go through `createHash` — every vector assertion is kept, but the
// "two surfaces agree" claim is gone, because there is only one surface left.
//
// One assertion changed meaning rather than spelling: `hash_new("md5")` used to
// mean "unsupported algorithm — invalid handle, digest length -1". `md5` is a
// perfectly ordinary algorithm to Node, so that case now pins Node's md5
// digest, and the "algorithm we do not know" case is pinned separately, on the
// name `nosuchalg`, with Node's real behaviour: `createHash` THROWS.

import { describe, test, expect } from "rts:test";
import { createHash } from "node:crypto";

// Official vector: sha256("hello world")
const KNOWN_HEX =
    "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
const KNOWN_B64 = "uU0nuZNNPgilLlLX2n2r+sSE7+N6U4DukIj3rOLvzek=";
const KNOWN_EMPTY =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

// Official vector: md5("hello world") — was the "unknown algorithm" case.
const KNOWN_MD5 = "5eb63bbbe01eeed093cb22bb8f5acdc3";

// 1. One-shot via a single update
const h1 = createHash("sha256");
h1.update("hello world");
const hex1 = h1.digest("hex");

// 2. Incremental across three updates — must equal the one-shot
const h2 = createHash("sha256");
h2.update("hello");
h2.update(" ");
h2.update("world");
const hex2 = h2.digest("hex");

// 3. Empty input
const h3 = createHash("sha256");
const hex3 = h3.digest("hex");

// 4. Base64 digest
const h4 = createHash("sha256");
h4.update("hello world");
const b64 = h4.digest("base64");

// 5. `md5` is a supported algorithm here, not an unknown one — so it is pinned
//    to its real digest. The unknown-algorithm case moved to `nosuchalg`, where
//    Node's contract is a THROW from `createHash`, not a sentinel value.
const md5hex = createHash("md5").update("hello world").digest("hex");
let unknownThrew = false;
try {
  createHash("nosuchalg").update("x").digest("hex");
} catch (e) {
  unknownThrew = true;
}

// 6. node:crypto hex — the object surface over the same primitive
const hex6 = createHash("sha256").update("hello world").digest("hex");

// 7. node:crypto base64
const b64_7 = createHash("sha256").update("hello world").digest("base64");

// 8. node:crypto incremental, to prove `.update()` chains like the handle API
const h8 = createHash("sha256");
h8.update("hello");
h8.update(" ");
h8.update("world");
const hex8 = h8.digest("hex");

describe("crypto_streaming_hash", () => {
    test("sha256(hello world) one-shot hex", () =>
        expect(hex1).toBe(KNOWN_HEX));
    test("sha256(hello world) incremental matches one-shot", () =>
        expect(hex2).toBe(KNOWN_HEX));
    test("sha256(empty) hex", () => expect(hex3).toBe(KNOWN_EMPTY));
    test("sha256(hello world) base64 is 44 chars", () =>
        expect(b64.length).toBe(44));
    test("md5 is supported and matches its official vector", () =>
        expect(md5hex).toBe(KNOWN_MD5));
    test("an unknown algorithm throws", () =>
        expect(unknownThrew).toBe(true));
    test("node:crypto createHash hex matches", () =>
        expect(hex6).toBe(KNOWN_HEX));
    test("node:crypto createHash base64 known", () =>
        expect(b64_7).toBe(KNOWN_B64));
    test("chained .update() matches the stepwise form", () =>
        expect(hex8).toBe(hex1));
});
