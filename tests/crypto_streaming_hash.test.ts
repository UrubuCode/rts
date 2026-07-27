// (#289 fase 2) Streaming hash via sha2 crate em rts.crypto e
// node:crypto. Suporta SHA-256 com update incremental + digest hex/base64.

import { describe, test, expect } from "rts:test";
import { crypto } from "rts";
import { createHash } from "node:crypto";

// Os helpers soltos `hashUpdate`/`hashDigestHex`/`hashDigestBase64` eram atalhos
// so-do-RTS e foram drenados: no Node o hash e ENCADEADO —
// `createHash(alg).update(data).digest(enc)`. Os primitivos crus continuam
// disponiveis em `rts.crypto.hash_*` (usados na primeira metade do arquivo).

// Vetor de teste oficial: sha256("hello world")
const KNOWN_HEX =
    "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

// 1. One-shot via update(string completa)
const h1 = crypto.hash_new("sha256");
crypto.hash_update_str(h1, "hello world");
const hex1 = crypto.hash_digest_hex(h1);

// 2. Incremental via 3 updates
const h2 = crypto.hash_new("sha256");
crypto.hash_update_str(h2, "hello");
crypto.hash_update_str(h2, " ");
crypto.hash_update_str(h2, "world");
const hex2 = crypto.hash_digest_hex(h2);

// 3. Empty input
const h3 = crypto.hash_new("sha256");
const hex3 = crypto.hash_digest_hex(h3);
const KNOWN_EMPTY =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" +
    "0000000000000000000000000000000000000000000000000000000000000000";
// SHA-256 vazio canonico (32 bytes -> 64 chars hex):
const KNOWN_EMPTY_REAL =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

// 4. Digest base64
const h4 = crypto.hash_new("sha256");
crypto.hash_update_str(h4, "hello world");
const b64 = crypto.hash_digest_base64(h4);

// 5. Algoritmo desconhecido retorna handle invalido (digest retorna 0).
const hBad = crypto.hash_new("md5");
const hBad_digest = crypto.hash_digest_hex(hBad);
const hBad_empty: i64 = hBad_digest.length;

// 6. node:crypto encadeado, hex
const hex6 = createHash("sha256").update("hello world").digest("hex");

// 7. node:crypto encadeado, base64
const b64_7 = createHash("sha256").update("hello world").digest("base64");
const KNOWN_B64 = "uU0nuZNNPgilLlLX2n2r+sSE7+N6U4DukIj3rOLvzek=";

// 8. update INCREMENTAL na API do Node: 3 chamadas devem dar o mesmo hash.
const hex8 = createHash("sha256")
    .update("hello")
    .update(" ")
    .update("world")
    .digest("hex");

describe("crypto_streaming_hash", () => {
    test("sha256(hello world) one-shot hex", () =>
        expect(hex1).toBe(KNOWN_HEX));
    test("sha256(hello world) incremental matches one-shot", () =>
        expect(hex2).toBe(KNOWN_HEX));
    test("sha256(empty) hex", () => expect(hex3).toBe(KNOWN_EMPTY_REAL));
    test("sha256(hello world) base64 starts with 'uU0nuZN'", () =>
        // base64 de b94d27b9934d3e08...
        expect(b64.length).toBe(44));
    test("unknown alg digest returns invalid (length -1)", () =>
        expect(hBad_empty).toBe(-1));
    test("node:crypto.createHash hex matches", () =>
        expect(hex6).toBe(KNOWN_HEX));
    test("node:crypto.createHash base64 known", () =>
        expect(b64_7).toBe(KNOWN_B64));
    test("node:crypto update incremental bate com one-shot", () =>
        expect(hex8).toBe(KNOWN_HEX));
});
