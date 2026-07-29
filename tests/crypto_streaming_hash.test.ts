// Streaming SHA-256 — the low-level `rts.crypto` handle surface AND the
// `node:crypto` object surface, checked against each other and against the
// official vectors.
//
// The `node:crypto` half used to import flat `hashUpdate`/`hashDigestHex`/
// `hashDigestBase64` helpers from the pre-`node:` era. Those no longer exist —
// `createHash` now returns a real Hash OBJECT with `.update()`/`.digest()` — so
// the file bailed at import. Rewritten onto the real API; the `rts.crypto`
// handle half is unchanged, and the point of the file (both surfaces agree on
// the same digest) is preserved.

import { describe, test, expect } from "rts:test";
import { crypto } from "rts";
import { createHash } from "node:crypto";

// Official vector: sha256("hello world")
const KNOWN_HEX =
    "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
const KNOWN_B64 = "uU0nuZNNPgilLlLX2n2r+sSE7+N6U4DukIj3rOLvzek=";
const KNOWN_EMPTY =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

// 1. One-shot via a single update
const h1 = crypto.hash_new("sha256");
crypto.hash_update_str(h1, "hello world");
const hex1 = crypto.hash_digest_hex(h1);

// 2. Incremental across three updates — must equal the one-shot
const h2 = crypto.hash_new("sha256");
crypto.hash_update_str(h2, "hello");
crypto.hash_update_str(h2, " ");
crypto.hash_update_str(h2, "world");
const hex2 = crypto.hash_digest_hex(h2);

// 3. Empty input
const h3 = crypto.hash_new("sha256");
const hex3 = crypto.hash_digest_hex(h3);

// 4. Base64 digest
const h4 = crypto.hash_new("sha256");
crypto.hash_update_str(h4, "hello world");
const b64 = crypto.hash_digest_base64(h4);

// 5. An unknown algorithm yields an invalid handle (digest reads back empty)
const hBad = crypto.hash_new("md5");
const hBad_digest = crypto.hash_digest_hex(hBad);
const hBad_empty: i64 = hBad_digest.length;

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
    test("unknown alg digest returns invalid (length -1)", () =>
        expect(hBad_empty).toBe(-1));
    test("node:crypto createHash hex matches", () =>
        expect(hex6).toBe(KNOWN_HEX));
    test("node:crypto createHash base64 known", () =>
        expect(b64_7).toBe(KNOWN_B64));
    test("node:crypto incremental matches the handle API", () =>
        expect(hex8).toBe(hex1));
});
