import { describe, test, expect } from "rts:test";
import {
  generateX25519KeyPair,
  x25519PublicKey,
  xeddsaSign,
  xeddsaVerify,
  createCipheriv,
  createDecipheriv,
  getCiphers,
  randomBytes,
} from "node:crypto";

// XEdDSA — Signal's signature over an X25519 identity key. The private key is
// read straight off the pair and the PUBLIC key is derived with
// `x25519PublicKey` rather than read off the same object: a field read off an
// ad-hoc shaped object is not statically proven array-typed by this engine,
// which is a separate emitter gap `crypto/curve/mod.rs` names.
const identity = generateX25519KeyPair();
const identityPriv = identity.privateKey;
const identityPub = x25519PublicKey(identityPriv);

const message = Buffer.from("signedPreKey payload");
const signature = xeddsaSign(identityPriv, message);
const signatureLength = Buffer.from(signature).length;
const verified = xeddsaVerify(identityPub, message, signature);

// A different message under the same signature must not verify.
const otherMessage = Buffer.from("signedPreKey payloae");
const verifiedOther = xeddsaVerify(identityPub, otherMessage, signature);

// A different key must not verify either.
const stranger = generateX25519KeyPair();
const strangerPub = x25519PublicKey(stranger.privateKey);
const verifiedStranger = xeddsaVerify(strangerPub, message, signature);

// A corrupted signature must not verify. `signature` is a Buffer, so a copy
// through an array is how one byte gets flipped without mutating the original.
const tampered = Buffer.from(signature);
tampered[0] = tampered[0] ^ 1;
const verifiedTampered = xeddsaVerify(identityPub, message, tampered);

// Malformed input answers false rather than throwing — every caller is checking
// something that arrived over a network.
const verifiedShort = xeddsaVerify(identityPub, message, randomBytes(63));

// The signature is randomized (the scheme mixes 64 random bytes into the
// nonce), so two signatures over one key and one message differ — and both
// verify. A deterministic answer here would mean the randomness was dropped.
const again = xeddsaSign(identityPriv, message);
const signaturesDiffer =
  Buffer.from(again).toString("hex") !== Buffer.from(signature).toString("hex");
const verifiedAgain = xeddsaVerify(identityPub, message, again);

// getCiphers() names exactly what createCipheriv accepts. This used to answer
// an empty list because nothing backed any name.
const ciphers = getCiphers();
const cipherCount = ciphers.length;
const namesAes256Gcm = ciphers.indexOf("aes-256-gcm") >= 0;

// setAAD binds a header to a message: the same ciphertext and tag must fail to
// authenticate under different associated data.
const aadKey = randomBytes(32);
const aadIv = randomBytes(12);
const aadCipher = createCipheriv("aes-256-gcm", aadKey, aadIv);
aadCipher.setAAD(Buffer.from("header-v1"));
aadCipher.update(Buffer.from("body"));
const aadCt = aadCipher.final();
const aadTag = aadCipher.getAuthTag();

const aadGood = createDecipheriv("aes-256-gcm", aadKey, aadIv);
aadGood.setAAD(Buffer.from("header-v1"));
aadGood.setAuthTag(aadTag);
aadGood.update(aadCt);
const aadPlain = Buffer.from(aadGood.final()).toString("hex");
const aadExpected = Buffer.from("body").toString("hex");

let aadMismatchThrew = false;
try {
  const aadBad = createDecipheriv("aes-256-gcm", aadKey, aadIv);
  aadBad.setAAD(Buffer.from("header-v2"));
  aadBad.setAuthTag(aadTag);
  aadBad.update(aadCt);
  aadBad.final();
} catch (e) {
  aadMismatchThrew = true;
}

// A key of the wrong length is refused at createCipheriv, not several calls
// later inside a function that never mentions the algorithm.
let shortKeyThrew = false;
try {
  createCipheriv("aes-256-gcm", randomBytes(16), randomBytes(12));
} catch (e) {
  shortKeyThrew = true;
}

let unknownAlgoThrew = false;
try {
  createCipheriv("aes-256-ctr", randomBytes(32), randomBytes(16));
} catch (e) {
  unknownAlgoThrew = true;
}

// Buffer.concat([update(), final()]) is the call this module has to stay
// correct under — update() answers an empty Buffer here rather than partial
// ciphertext, and the concatenation is the same bytes either way.
const concatKey = randomBytes(32);
const concatIv = randomBytes(16);
const concatCipher = createCipheriv("aes-256-cbc", concatKey, concatIv);
const concatCt = Buffer.concat([
  concatCipher.update(Buffer.from("streamed in one go")),
  concatCipher.final(),
]);
const concatDecipher = createDecipheriv("aes-256-cbc", concatKey, concatIv);
const concatPt = Buffer.concat([concatDecipher.update(concatCt), concatDecipher.final()]);
const concatHex = Buffer.from(concatPt).toString("hex");
const concatExpected = Buffer.from("streamed in one go").toString("hex");

describe("node:crypto XEdDSA", () => {
  test("a signature is 64 bytes", () => {
    expect(signatureLength).toBe(64);
  });

  test("a signature verifies under its own key and message", () => {
    expect(verified).toBe(true);
  });

  test("a different message does not verify", () => {
    expect(verifiedOther).toBe(false);
  });

  test("a different key does not verify", () => {
    expect(verifiedStranger).toBe(false);
  });

  test("a tampered signature does not verify", () => {
    expect(verifiedTampered).toBe(false);
  });

  test("a malformed signature answers false rather than throwing", () => {
    expect(verifiedShort).toBe(false);
  });

  test("signing twice gives two different signatures, both valid", () => {
    expect(signaturesDiffer).toBe(true);
    expect(verifiedAgain).toBe(true);
  });
});

describe("node:crypto cipher surface", () => {
  test("getCiphers names what createCipheriv accepts", () => {
    expect(cipherCount).toBe(4);
    expect(namesAes256Gcm).toBe(true);
  });

  test("setAAD authenticates the associated data", () => {
    expect(aadPlain).toBe(aadExpected);
    expect(aadMismatchThrew).toBe(true);
  });

  test("a wrong key length is refused at construction", () => {
    expect(shortKeyThrew).toBe(true);
  });

  test("an unsupported algorithm is refused by name", () => {
    expect(unknownAlgoThrew).toBe(true);
  });

  test("Buffer.concat of update and final round-trips", () => {
    expect(concatHex).toBe(concatExpected);
  });
});
