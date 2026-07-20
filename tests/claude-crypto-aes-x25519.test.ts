import { describe, test, expect } from "rts:test";
import { createCipheriv, createDecipheriv, generateX25519KeyPair, x25519PublicKey, x25519DiffieHellman, randomBytes } from "node:crypto";

// AES-256-GCM round-trip. update() only accumulates; final() returns the
// full result (RTS's Cipher model — see mod.rs doc comment).
const gcmKey = randomBytes(32);
const gcmIv = randomBytes(12);
const gcmPlain = Buffer.from("hello baileys");
const gcmCipher = createCipheriv("aes-256-gcm", gcmKey, gcmIv);
gcmCipher.update(gcmPlain);
const gcmCt = gcmCipher.final();
const gcmTag = gcmCipher.getAuthTag();
const gcmDecipher = createDecipheriv("aes-256-gcm", gcmKey, gcmIv);
gcmDecipher.setAuthTag(gcmTag);
gcmDecipher.update(gcmCt);
const gcmPt = gcmDecipher.final();
// Buffer.from(numberArray).toString() has a pre-existing, unrelated gap (does
// not decode bytes as UTF-8 text) — compare hex instead, which works.
const gcmRoundTripHex = Buffer.from(gcmPt).toString("hex");
const gcmPlainHex = Buffer.from(gcmPlain).toString("hex");

// AES-256-GCM wrong tag → throws.
const badKey = randomBytes(32);
const badIv = randomBytes(12);
const badCipher = createCipheriv("aes-256-gcm", badKey, badIv);
badCipher.update(Buffer.from("data"));
const badCt = badCipher.final();
let gcmThrew = false;
const badDecipher = createDecipheriv("aes-256-gcm", badKey, badIv);
badDecipher.setAuthTag(randomBytes(16));
badDecipher.update(badCt);
try {
  badDecipher.final();
} catch (e) {
  gcmThrew = true;
}

// AES-256-CBC round-trip.
const cbcKey = randomBytes(32);
const cbcIv = randomBytes(16);
const cbcPlain = Buffer.from("whatsapp binary node payload");
const cbcCipher = createCipheriv("aes-256-cbc", cbcKey, cbcIv);
cbcCipher.update(cbcPlain);
const cbcCt = cbcCipher.final();
const cbcDecipher = createDecipheriv("aes-256-cbc", cbcKey, cbcIv);
cbcDecipher.update(cbcCt);
const cbcPt = cbcDecipher.final();
const cbcRoundTripHex = Buffer.from(cbcPt).toString("hex");
const cbcPlainHex = Buffer.from(cbcPlain).toString("hex");

// X25519 shared secret.
const alice = generateX25519KeyPair();
const bob = generateX25519KeyPair();
const sharedA = x25519DiffieHellman(alice.privateKey, bob.publicKey);
const sharedB = x25519DiffieHellman(bob.privateKey, alice.publicKey);
const sharedAHex = Buffer.from(sharedA).toString("hex");
const sharedBHex = Buffer.from(sharedB).toString("hex");

// X25519 public-key derivation.
const kp = generateX25519KeyPair();
const derived = x25519PublicKey(kp.privateKey);
const derivedHex = Buffer.from(derived).toString("hex");
const kpPubHex = Buffer.from(kp.publicKey).toString("hex");

describe("node:crypto AES-GCM/CBC + X25519", () => {
  test("AES-256-GCM round-trips", () => {
    expect(gcmRoundTripHex).toBe(gcmPlainHex);
  });

  test("AES-256-GCM wrong tag throws", () => {
    expect(gcmThrew).toBe(true);
  });

  test("AES-256-CBC round-trips", () => {
    expect(cbcRoundTripHex).toBe(cbcPlainHex);
  });

  test("X25519 shared secret matches both directions", () => {
    expect(sharedAHex).toBe(sharedBHex);
  });

  test("X25519 public key derivation matches keypair generation", () => {
    expect(derivedHex).toBe(kpPubHex);
  });
});
