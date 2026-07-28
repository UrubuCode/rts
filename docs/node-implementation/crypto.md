# node:crypto

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:crypto` |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P1 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import crypto from "node:crypto"`; `import { createHash, randomUUID, ... } from "node:crypto"`; `import crypto from "crypto"` (bare specifier, legacy alias); `const crypto = require("node:crypto")` / `require("crypto")` |
| Globals exposed | `globalThis.crypto` — the Web Crypto `Crypto` singleton (`crypto.subtle`, `crypto.getRandomValues`, `crypto.randomUUID`) is available **without any import**, mirroring Node ≥19 / browser behavior. No other global is introduced by this module. |

## 1. Purpose

`node:crypto` exposes OpenSSL-class cryptographic primitives to JS/TS: hashing
(SHA-1/2/3, SHAKE, BLAKE-family via legacy names), symmetric ciphers (AES,
ChaCha20-Poly1305), asymmetric key management and signing (RSA, DSA, EC,
Ed25519/Ed448, X25519/X448, DH), key derivation (PBKDF2, scrypt, HKDF,
Argon2), CSPRNG-backed randomness, X.509 certificate inspection, and the
standards-track Web Crypto API (`crypto.webcrypto` / `crypto.subtle`). It is
the substrate every higher-level Node security API (`tls`, `https`, JWT
libraries, password hashing, `node:sqlite` encryption extensions, etc.) is
built on, so parity here directly gates parity elsewhere.

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `Certificate` (legacy, SPKAC)

Base class: none. Both a constructable class and callable-as-function
(`Certificate()` returns an instance without `new`, kept for legacy scripts).

| Member | Kind | Signature | Returns |
|---|---|---|---|
| `Certificate.exportChallenge` | static | `(spkac: BinaryLike, encoding?: string) => Buffer` | `Buffer` |
| `Certificate.exportPublicKey` | static | `(spkac: BinaryLike, encoding?: string) => Buffer` | `Buffer` |
| `Certificate.verifySpkac` | static | `(spkac: BinaryLike, encoding?: string) => boolean` | `boolean` |
| `certificate.exportChallenge` | instance | same as static | `Buffer` |
| `certificate.exportPublicKey` | instance | same as static | `Buffer` |
| `certificate.verifySpkac` | instance | same as static | `boolean` |

No instance properties, no events.

#### `Cipheriv` / `Decipheriv`

Base class: `stream.Transform`. Not directly constructible — only produced by
`crypto.createCipheriv()` / `crypto.createDecipheriv()`. (The legacy IV-less
`Cipher`/`Decipher` obtained from `createCipher()`/`createDecipher()` are
**removed** — see §4.)

| Method | Signature | Returns | Notes |
|---|---|---|---|
| `cipher.update` | `(data: BinaryLike, inputEncoding?: Encoding, outputEncoding?: Encoding) => Buffer \| string` | `Buffer \| string` | May be called multiple times |
| `cipher.final` | `(outputEncoding?: Encoding) => Buffer \| string` | `Buffer \| string` | Ends the stream; throws if called twice |
| `cipher.setAAD` | `(buffer: BinaryLike, options?: { plaintextLength?: number, encoding?: Encoding }) => Cipheriv` | `this` | AEAD modes only (GCM/CCM/OCB/ChaCha20-Poly1305) |
| `cipher.getAuthTag` | `() => Buffer` | `Buffer` | AEAD modes only; call after `final()` |
| `cipher.setAutoPadding` | `(autoPadding?: boolean = true) => Cipheriv` | `this` | Block ciphers only |
| `decipher.setAuthTag` | `(buffer: BinaryLike, encoding?: Encoding) => Decipheriv` | `this` | AEAD modes only; call before `final()` |

Decipheriv has the same method set as Cipheriv minus `getAuthTag` (replaced by
`setAuthTag`).

Events (inherited from `stream.Transform`/`Duplex`): `'data'`, `'end'`,
`'error'`, `'close'`, `'drain'`, `'finish'`, `'pipe'`, `'unpipe'`.

#### `DiffieHellman`

Base class: none. Constructed via `crypto.createDiffieHellman(...)`.

| Method | Signature | Returns |
|---|---|---|
| `diffieHellman.computeSecret` | `(otherPublicKey: BinaryLike, inputEncoding?: Encoding, outputEncoding?: Encoding) => Buffer \| string` | `Buffer \| string` |
| `diffieHellman.generateKeys` | `(encoding?: Encoding) => Buffer \| string` | `Buffer \| string` |
| `diffieHellman.getGenerator` | `(encoding?: Encoding) => Buffer \| string` | `Buffer \| string` |
| `diffieHellman.getPrime` | `(encoding?: Encoding) => Buffer \| string` | `Buffer \| string` |
| `diffieHellman.getPrivateKey` | `(encoding?: Encoding) => Buffer \| string` | `Buffer \| string` |
| `diffieHellman.getPublicKey` | `(encoding?: Encoding) => Buffer \| string` | `Buffer \| string` |
| `diffieHellman.setPrivateKey` | `(privateKey: BinaryLike, encoding?: Encoding) => void` | `void` |
| `diffieHellman.setPublicKey` | `(publicKey: BinaryLike, encoding?: Encoding) => void` | `void` |

Instance property: `diffieHellman.verifyError: number` (bit field of
`DH_CHECK_P_NOT_SAFE_PRIME` / `DH_CHECK_P_NOT_PRIME` / `DH_NOT_SUITABLE_GENERATOR`
etc — see `crypto.constants`). No events.

#### `DiffieHellmanGroup`

Base class: none (same shape as `DiffieHellman`). Constructed via
`crypto.createDiffieHellmanGroup(name)` / `crypto.getDiffieHellman(name)`.
Same methods as `DiffieHellman` **except** `setPrivateKey`/`setPublicKey` are
not implemented (throw). Has `verifyError`. No events.

#### `ECDH`

Base class: none. Constructed via `crypto.createECDH(curveName)`.

| Member | Kind | Signature | Returns |
|---|---|---|---|
| `ECDH.convertKey` | static | `(key: BinaryLike, curve: string, inputEncoding?: Encoding, outputEncoding?: Encoding, format?: 'compressed' \| 'uncompressed' \| 'hybrid') => Buffer \| string` | `Buffer \| string` |
| `ecdh.computeSecret` | instance | `(otherPublicKey: BinaryLike, inputEncoding?: Encoding, outputEncoding?: Encoding) => Buffer \| string` | `Buffer \| string` |
| `ecdh.generateKeys` | instance | `(encoding?: Encoding, format?: ECDHKeyFormat = 'uncompressed') => Buffer \| string` | `Buffer \| string` |
| `ecdh.getPrivateKey` | instance | `(encoding?: Encoding) => Buffer \| string` | `Buffer \| string` |
| `ecdh.getPublicKey` | instance | `(encoding?: Encoding, format?: ECDHKeyFormat = 'uncompressed') => Buffer \| string` | `Buffer \| string` |
| `ecdh.setPrivateKey` | instance | `(privateKey: BinaryLike, encoding?: Encoding) => void` | `void` |
| `ecdh.setPublicKey` (deprecated, no-op-ish) | instance | `(publicKey: BinaryLike, encoding?: Encoding) => void` | `void` |

No instance properties beyond internal state, no events.

#### `Hash`

Base class: none (implements `stream.Transform`-like `pipe`, but documented
as a plain class). Constructed via `crypto.createHash(algorithm[, options])`.

| Method | Signature | Returns |
|---|---|---|
| `hash.update` | `(data: BinaryLike, inputEncoding?: Encoding) => Hash` | `this` (chainable) |
| `hash.digest` | `(encoding?: Encoding) => Buffer \| string` | `Buffer` if no encoding, else `string` |
| `hash.copy` | `(options?: HashOptions) => Hash` | new `Hash` with identical internal state |

No instance properties, no events.

#### `Hmac`

Base class: none. Constructed via `crypto.createHmac(algorithm, key[, options])`.

| Method | Signature | Returns |
|---|---|---|
| `hmac.update` | `(data: BinaryLike, inputEncoding?: Encoding) => Hmac` | `this` (chainable) |
| `hmac.digest` | `(encoding?: Encoding) => Buffer \| string` | `Buffer` if no encoding, else `string` |

No instance properties, no events.

#### `KeyObject`

Base class: none. Never constructed with `new` — obtained from
`crypto.createPrivateKey()`, `crypto.createPublicKey()`,
`crypto.createSecretKey()`, `crypto.generateKeyPair(Sync)`,
`crypto.generateKey(Sync)`, or `KeyObject.from()`.

| Member | Kind | Signature | Returns |
|---|---|---|---|
| `KeyObject.from` | static | `(key: CryptoKey) => KeyObject` | `KeyObject` |
| `keyObject.asymmetricKeyDetails` | property (readonly) | `AsymmetricKeyDetails \| undefined` | — |
| `keyObject.asymmetricKeyType` | property (readonly) | `'rsa' \| 'rsa-pss' \| 'dsa' \| 'ec' \| 'ed25519' \| 'ed448' \| 'x25519' \| 'x448' \| 'dh' \| undefined` | — |
| `keyObject.symmetricKeySize` | property (readonly) | `number \| undefined` | — |
| `keyObject.type` | property (readonly) | `'secret' \| 'public' \| 'private'` | — |
| `keyObject.equals` | instance | `(otherKeyObject: KeyObject) => boolean` | `boolean` |
| `keyObject.export` | instance | `(options?: KeyExportOptions) => string \| Buffer \| JsonWebKey` | see §3 |
| `keyObject.toCryptoKey` | instance | `(algorithm: string \| object, extractable: boolean, keyUsages: KeyUsage[]) => CryptoKey` | `CryptoKey` |

No events.

#### `Sign`

Base class: `stream.Writable`. Constructed via
`crypto.createSign(algorithm[, options])`.

| Method | Signature | Returns |
|---|---|---|
| `sign.update` | `(data: BinaryLike, inputEncoding?: Encoding) => Sign` | `this` |
| `sign.sign` | `(privateKey: KeyLike \| SignPrivateKeyInput, outputEncoding?: Encoding) => Buffer \| string` | `Buffer` or `string` |

Events (inherited `Writable`): `'finish'`, `'error'`, `'close'`, `'drain'`,
`'pipe'`, `'unpipe'`.

#### `Verify`

Base class: `stream.Writable`. Constructed via
`crypto.createVerify(algorithm[, options])`.

| Method | Signature | Returns |
|---|---|---|
| `verify.update` | `(data: BinaryLike, inputEncoding?: Encoding) => Verify` | `this` |
| `verify.verify` | `(object: KeyLike \| VerifyPublicKeyInput, signature: BinaryLike, signatureEncoding?: Encoding) => boolean` | `boolean` |

Events: same as `Sign`.

#### `X509Certificate`

Base class: none.

| Member | Kind | Signature | Returns |
|---|---|---|---|
| constructor | — | `new X509Certificate(buffer: BinaryLike)` | — |
| `x509.ca` | property | `boolean` | — |
| `x509.fingerprint` | property | `string` (SHA-1) | — |
| `x509.fingerprint256` | property | `string` (SHA-256) | — |
| `x509.fingerprint512` | property | `string` (SHA-512) | — |
| `x509.infoAccess` | property | `string \| undefined` | — |
| `x509.issuer` | property | `string` | — |
| `x509.issuerCertificate` | property | `X509Certificate \| undefined` | — |
| `x509.keyUsage` | property | `string[]` | — |
| `x509.publicKey` | property | `KeyObject` | — |
| `x509.raw` | property | `Buffer` (DER) | — |
| `x509.serialNumber` | property | `string` (hex) | — |
| `x509.subject` | property | `string` | — |
| `x509.subjectAltName` | property | `string \| undefined` | — |
| `x509.validFrom` | property | `string` | — |
| `x509.validFromDate` | property | `Date` | — |
| `x509.validTo` | property | `string` | — |
| `x509.validToDate` | property | `Date` | — |
| `x509.signatureAlgorithm` | property | `string` | — |
| `x509.checkEmail` | instance | `(email: string, options?: X509CheckOptions) => string \| undefined` | matching SAN or `undefined` |
| `x509.checkHost` | instance | `(name: string, options?: X509CheckOptions) => string \| undefined` | matching SAN or `undefined` |
| `x509.checkIP` | instance | `(ip: string) => string \| undefined` | matching SAN or `undefined` |
| `x509.checkIssued` | instance | `(otherCert: X509Certificate) => boolean` | `boolean` |
| `x509.checkPrivateKey` | instance | `(privateKey: KeyObject) => boolean` | `boolean` |
| `x509.verify` | instance | `(publicKey: KeyObject) => boolean` | `boolean` |
| `x509.toJSON` | instance | `() => string` (PEM) | `string` |
| `x509.toLegacyObject` | instance | `() => object` (legacy `tls.peerCertificate`-shaped) | `object` |
| `x509.toString` | instance | `() => string` (PEM) | `string` |

No events.

#### `Crypto` (Web Crypto global)

Base class: none. Singleton at `crypto.webcrypto` and `globalThis.crypto`.

| Member | Kind | Signature | Returns |
|---|---|---|---|
| `crypto.subtle` | property (readonly) | `SubtleCrypto` | — |
| `crypto.getRandomValues` | instance | `<T extends ArrayBufferView>(typedArray: T) => T` | fills and returns `typedArray` (max 65 536 bytes, integer typed arrays only) |
| `crypto.randomUUID` | instance | `(options?: { disableEntropyCache?: boolean }) => string` | RFC 4122 v4 UUID |
| `crypto.CryptoKey` | property | constructor reference (not directly constructible) | — |

#### `SubtleCrypto`

Base class: none. Accessed via `crypto.subtle` / `crypto.webcrypto.subtle`.
Every method returns a `Promise` (never throws synchronously except for
programmer-error `TypeError`s on malformed arguments).

| Method | Signature | Returns |
|---|---|---|
| `subtle.encrypt` | `(algorithm: AlgorithmIdentifier \| RsaOaepParams \| AesCtrParams \| AesCbcParams \| AesGcmParams, key: CryptoKey, data: BufferSource) => Promise<ArrayBuffer>` | ciphertext |
| `subtle.decrypt` | same params as `encrypt` | `Promise<ArrayBuffer>` plaintext |
| `subtle.sign` | `(algorithm: AlgorithmIdentifier \| RsaPssParams \| EcdsaParams, key: CryptoKey, data: BufferSource) => Promise<ArrayBuffer>` | signature |
| `subtle.verify` | `(algorithm: ..., key: CryptoKey, signature: BufferSource, data: BufferSource) => Promise<boolean>` | valid? |
| `subtle.digest` | `(algorithm: AlgorithmIdentifier, data: BufferSource) => Promise<ArrayBuffer>` | hash |
| `subtle.generateKey` | `(algorithm: AlgorithmIdentifier \| RsaHashedKeyGenParams \| EcKeyGenParams \| HmacKeyGenParams \| AesKeyGenParams, extractable: boolean, keyUsages: KeyUsage[]) => Promise<CryptoKey \| CryptoKeyPair>` | key(s) |
| `subtle.deriveKey` | `(algorithm: EcdhKeyDeriveParams \| HkdfParams \| Pbkdf2Params, baseKey: CryptoKey, derivedKeyAlgorithm: AlgorithmIdentifier, extractable: boolean, keyUsages: KeyUsage[]) => Promise<CryptoKey>` | key |
| `subtle.deriveBits` | `(algorithm: EcdhKeyDeriveParams \| HkdfParams \| Pbkdf2Params, baseKey: CryptoKey, length?: number \| null) => Promise<ArrayBuffer>` | bits |
| `subtle.importKey` | `(format: KeyFormat, keyData: BufferSource \| JsonWebKey, algorithm: AlgorithmIdentifier, extractable: boolean, keyUsages: KeyUsage[]) => Promise<CryptoKey>` | key |
| `subtle.exportKey` | `(format: KeyFormat, key: CryptoKey) => Promise<ArrayBuffer \| JsonWebKey>` | exported material |
| `subtle.wrapKey` | `(format: KeyFormat, key: CryptoKey, wrappingKey: CryptoKey, wrapAlgo: AlgorithmIdentifier) => Promise<ArrayBuffer>` | wrapped bytes |
| `subtle.unwrapKey` | `(format: KeyFormat, wrappedKey: BufferSource, unwrappingKey: CryptoKey, unwrapAlgo: AlgorithmIdentifier, unwrappedKeyAlgorithm: AlgorithmIdentifier, extractable: boolean, keyUsages: KeyUsage[]) => Promise<CryptoKey>` | key |
| `subtle.getPublicKey` *(Node ext.)* | `(key: CryptoKey, keyUsages: KeyUsage[]) => Promise<CryptoKey>` | derives public key from a private key |
| `subtle.encapsulateBits` *(Node ext., ML-KEM)* | `(encapsulationAlgorithm: AlgorithmIdentifier, encapsulationKey: CryptoKey) => Promise<{ciphertext: ArrayBuffer, sharedKey: ArrayBuffer}>` | KEM output |
| `subtle.encapsulateKey` *(Node ext.)* | `(encapsulationAlgorithm, encapsulationKey: CryptoKey, sharedKeyAlgorithm, extractable, usages) => Promise<{ciphertext: ArrayBuffer, sharedKey: CryptoKey}>` | KEM output |
| `subtle.decapsulateBits` *(Node ext.)* | `(decapsulationAlgorithm, decapsulationKey: CryptoKey, ciphertext: BufferSource) => Promise<ArrayBuffer>` | shared secret bits |
| `subtle.decapsulateKey` *(Node ext.)* | `(decapsulationAlgorithm, decapsulationKey: CryptoKey, ciphertext, sharedKeyAlgorithm, extractable, usages) => Promise<CryptoKey>` | shared secret key |
| `SubtleCrypto.supports` *(Node ext., static)* | `(operation: string, algorithm: AlgorithmIdentifier, lengthOrAdditionalAlgorithm?: number \| AlgorithmIdentifier) => boolean` | feature-detect, synchronous |

#### `CryptoKey`

Base class: none. Never constructed directly.

| Property | Type |
|---|---|
| `cryptoKey.algorithm` | `KeyAlgorithm \| RsaHashedKeyAlgorithm \| EcKeyAlgorithm \| AesKeyAlgorithm \| HmacKeyAlgorithm` (readonly) |
| `cryptoKey.extractable` | `boolean` (readonly) |
| `cryptoKey.type` | `'secret' \| 'private' \| 'public'` (readonly) |
| `cryptoKey.usages` | `KeyUsage[]` (readonly) |

No methods, no events.

### 2.2 Top-level functions

Grouped by area. "Variant" = sync (returns directly / throws) · callback
(Node-style `(err, result) => void`, last arg) · promise (Web Crypto only).

**Randomness**

| Function | Signature | Variant |
|---|---|---|
| `randomBytes` | `(size: number, callback?: (err: Error \| null, buf: Buffer) => void) => Buffer \| void` | sync / callback |
| `randomFill` | `(buffer: T, offsetOrCallback?: number \| Callback, sizeOrCallback?: number \| Callback, callback?: Callback) => void` | callback |
| `randomFillSync` | `(buffer: T, offset?: number, size?: number) => T` | sync |
| `randomInt` | `(minOrMax: number, maxOrCallback?: number \| Callback, callback?: Callback) => number \| void` | sync / callback |
| `randomUUID` | `(options?: RandomUUIDOptions) => string` | sync |
| `getRandomValues` | `<T extends ArrayBufferView>(typedArray: T) => T` | sync |
| `timingSafeEqual` | `(a: NodeJS.ArrayBufferView, b: NodeJS.ArrayBufferView) => boolean` | sync |

**Hashing / MAC (one-shot + streaming factories)**

| Function | Signature | Variant |
|---|---|---|
| `hash` | `(algorithm: string, data: BinaryLike, outputEncoding?: Encoding \| 'buffer') => Buffer \| string` | sync |
| `createHash` | `(algorithm: string, options?: HashOptions) => Hash` | sync (factory) |
| `createHmac` | `(algorithm: string, key: BinaryLike \| KeyObject \| CryptoKey, options?: HashOptions) => Hmac` | sync (factory) |
| `getHashes` | `() => string[]` | sync |

**Symmetric cipher (factories)**

| Function | Signature | Variant |
|---|---|---|
| `createCipheriv` | `(algorithm: string, key: BinaryLike \| KeyObject \| CryptoKey, iv: BinaryLike \| null, options?: CipherCCMOptions \| CipherGCMOptions \| CipherOCBOptions \| TransformOptions) => Cipheriv` | sync (factory) |
| `createDecipheriv` | `(algorithm: string, key: BinaryLike \| KeyObject \| CryptoKey, iv: BinaryLike \| null, options?: same) => Decipheriv` | sync (factory) |
| `getCipherInfo` | `(nameOrNid: string \| number, options?: { keyLength?: number, ivLength?: number }) => CipherInfo \| undefined` | sync |
| `getCiphers` | `() => string[]` | sync |
| `publicEncrypt` | `(key: RsaPublicKey \| KeyLike, buffer: BinaryLike) => Buffer` | sync |
| `privateDecrypt` | `(privateKey: RsaPrivateKey \| KeyLike, buffer: BinaryLike) => Buffer` | sync |
| `privateEncrypt` | `(privateKey: RsaPrivateKey \| KeyLike, buffer: BinaryLike) => Buffer` | sync |
| `publicDecrypt` | `(key: RsaPublicKey \| KeyLike, buffer: BinaryLike) => Buffer` | sync |

**Key management**

| Function | Signature | Variant |
|---|---|---|
| `createPrivateKey` | `(key: PrivateKeyInput \| BinaryLike \| JsonWebKey) => KeyObject` | sync |
| `createPublicKey` | `(key: PublicKeyInput \| BinaryLike \| KeyObject \| JsonWebKey) => KeyObject` | sync |
| `createSecretKey` | `(key: BinaryLike, encoding?: Encoding) => KeyObject` | sync |
| `generateKey` | `(type: 'hmac' \| 'aes', options: { length: number }, callback: (err: Error \| null, key: KeyObject) => void) => void` | callback |
| `generateKeySync` | `(type: 'hmac' \| 'aes', options: { length: number }) => KeyObject` | sync |
| `generateKeyPair` | `(type: KeyPairType, options: GenerateKeyPairOptions, callback: (err: Error \| null, publicKey: KeyObject \| string \| Buffer, privateKey: KeyObject \| string \| Buffer) => void) => void` | callback |
| `generateKeyPairSync` | `(type: KeyPairType, options: GenerateKeyPairOptions) => { publicKey: KeyObject \| string \| Buffer, privateKey: KeyObject \| string \| Buffer }` | sync |
| `getDiffieHellman` | `(groupName: string) => DiffieHellmanGroup` | sync (alias: `getDiffieHellmanGroup`) |
| `createDiffieHellman` | `(prime: BinaryLike, primeEncoding?: Encoding, generator?: number \| BinaryLike, generatorEncoding?: Encoding) => DiffieHellman` (overload: `(primeLength: number, generator?: number) => DiffieHellman`) | sync (factory) |
| `createDiffieHellmanGroup` | `(name: string) => DiffieHellmanGroup` (alias of `getDiffieHellman`) | sync (factory) |
| `createECDH` | `(curveName: string) => ECDH` | sync (factory) |
| `diffieHellman` | `(options: { privateKey: KeyObject \| CryptoKey, publicKey: KeyObject \| CryptoKey }, callback?: (err, secret: Buffer) => void) => Buffer \| void` | sync / callback |
| `getCurves` | `() => string[]` | sync |

**Key derivation**

| Function | Signature | Variant |
|---|---|---|
| `pbkdf2` | `(password: BinaryLike, salt: BinaryLike, iterations: number, keylen: number, digest: string, callback: (err: Error \| null, derivedKey: Buffer) => void) => void` | callback |
| `pbkdf2Sync` | `(password: BinaryLike, salt: BinaryLike, iterations: number, keylen: number, digest: string) => Buffer` | sync |
| `scrypt` | `(password: BinaryLike, salt: BinaryLike, keylen: number, options: ScryptOptions \| undefined, callback: (err: Error \| null, derivedKey: Buffer) => void) => void` | callback |
| `scryptSync` | `(password: BinaryLike, salt: BinaryLike, keylen: number, options?: ScryptOptions) => Buffer` | sync |
| `hkdf` | `(digest: string, ikm: BinaryLike \| KeyObject \| CryptoKey, salt: BinaryLike, info: BinaryLike, keylen: number, callback: (err: Error \| null, derivedKey: ArrayBuffer) => void) => void` | callback |
| `hkdfSync` | `(digest: string, ikm: BinaryLike \| KeyObject \| CryptoKey, salt: BinaryLike, info: BinaryLike, keylen: number) => ArrayBuffer` | sync |
| `argon2` *(experimental, OpenSSL ≥3.5)* | `(algorithm: 'argon2d' \| 'argon2i' \| 'argon2id', parameters: Argon2Options, callback: (err: Error \| null, result: Buffer) => void) => void` | callback |
| `argon2Sync` *(experimental)* | `(algorithm: string, parameters: Argon2Options) => Buffer` | sync |

**Signing**

| Function | Signature | Variant |
|---|---|---|
| `createSign` | `(algorithm: string, options?: stream.WritableOptions) => Sign` | sync (factory) |
| `createVerify` | `(algorithm: string, options?: stream.WritableOptions) => Verify` | sync (factory) |
| `sign` | `(algorithm: string \| null, data: BinaryLike, key: KeyLike \| SignKeyObjectInput \| SignPrivateKeyInput \| CryptoKey, callback?: (err: Error \| null, signature: Buffer) => void) => Buffer \| void` | sync / callback |
| `verify` | `(algorithm: string \| null, data: BinaryLike, key: KeyLike \| VerifyKeyObjectInput \| VerifyPublicKeyInput \| CryptoKey, signature: BinaryLike, callback?: (err: Error \| null, result: boolean) => void) => boolean \| void` | sync / callback |

**Primes**

| Function | Signature | Variant |
|---|---|---|
| `checkPrime` | `(candidate: bigint \| ArrayBuffer \| BinaryLike, options?: CheckPrimeOptions, callback: (err: Error \| null, result: boolean) => void) => void` | callback |
| `checkPrimeSync` | `(candidate: bigint \| ArrayBuffer \| BinaryLike, options?: CheckPrimeOptions) => boolean` | sync |
| `generatePrime` | `(size: number, options?: GeneratePrimeOptions, callback: (err: Error \| null, prime: ArrayBuffer \| bigint) => void) => void` | callback |
| `generatePrimeSync` | `(size: number, options?: GeneratePrimeOptions) => ArrayBuffer \| bigint` | sync |

**Key encapsulation (module-level convenience, mirrors `subtle.encapsulate*`)**

| Function | Signature | Variant |
|---|---|---|
| `encapsulate` | `(key: KeyLike \| CryptoKey, callback?: (err: Error \| null, result: { ciphertext: Buffer, sharedKey: Buffer }) => void) => Promise<{ciphertext: Buffer, sharedKey: Buffer}> \| void` | callback / promise |
| `decapsulate` | `(key: KeyLike \| CryptoKey, ciphertext: BinaryLike, callback?: (err: Error \| null, sharedKey: Buffer) => void) => Buffer \| void` | sync / callback |

**Misc / engine / FIPS**

| Function | Signature | Variant |
|---|---|---|
| `setEngine` | `(engine: string, flags?: number) => void` | sync |
| `getFips` | `() => 0 \| 1` | sync |
| `setFips` | `(bool: boolean) => void` | sync, throws if unsupported build |
| `secureHeapUsed` | `() => { total: number, used: number, utilization: number, min: number }` | sync |

Total distinct top-level functions documented: **54**.

### 2.3 Properties & constants

| Name | Type | Notes |
|---|---|---|
| `crypto.constants` | `object` | see below |
| `crypto.fips` | `boolean` | **deprecated** (DEP0093-class prop mirror; verify) — use `getFips()`/`setFips()` |
| `crypto.webcrypto` | `Crypto` | same object identity as `globalThis.crypto` |
| `crypto.subtle` | `SubtleCrypto` | convenience alias for `crypto.webcrypto.subtle` |
| `crypto.DEFAULT_ENCODING` | `string` | **removed** in modern Node — do not implement (legacy global default-encoding knob, gone since v10) |

**`crypto.constants` groups** (mirrors OpenSSL's `<openssl/*.h>` values Node
re-exports; treat as an opaque pass-through table rather than reimplementing
OpenSSL numerics from scratch):

- RSA padding: `RSA_PKCS1_PADDING`, `RSA_NO_PADDING`, `RSA_PKCS1_OAEP_PADDING`,
  `RSA_X931_PADDING`, `RSA_PKCS1_PSS_PADDING`, `RSA_PSS_SALTLEN_DIGEST`,
  `RSA_PSS_SALTLEN_MAX_SIGN`, `RSA_PSS_SALTLEN_AUTO`. (`RSA_SSLV23_PADDING`
  removed in recent OpenSSL/Node — do not add.)
- EC point conversion: `POINT_CONVERSION_COMPRESSED`,
  `POINT_CONVERSION_UNCOMPRESSED`, `POINT_CONVERSION_HYBRID`.
- DH check bits: `DH_CHECK_P_NOT_SAFE_PRIME`, `DH_CHECK_P_NOT_PRIME`,
  `DH_UNABLE_TO_CHECK_GENERATOR`, `DH_NOT_SUITABLE_GENERATOR`,
  `DH_CHECK_P_NOT_SAFE_PRIME`.
- Engine method flags (legacy, OpenSSL ENGINE API):
  `ENGINE_METHOD_RSA`, `ENGINE_METHOD_DSA`, `ENGINE_METHOD_DH`,
  `ENGINE_METHOD_RAND`, `ENGINE_METHOD_EC`, `ENGINE_METHOD_CIPHERS`,
  `ENGINE_METHOD_DIGESTS`, `ENGINE_METHOD_PKEY_METHS`,
  `ENGINE_METHOD_PKEY_ASN1_METHS`, `ENGINE_METHOD_ALL`, `ENGINE_METHOD_NONE`.
- Cipher lists (strings): `defaultCoreCipherList`, `defaultCipherList`.
- Version/info: `OPENSSL_VERSION_NUMBER`.
- Long tail (~100+ entries): `SSL_OP_*` protocol/behavior flags,
  `ALPN_ENABLED`, `TLS1_VERSION`/`TLS1_1_VERSION`/`TLS1_2_VERSION`/
  `TLS1_3_VERSION`, `SSL_OP_ALL`, `SSL_OP_NO_SSLv2/3`, etc. — RTS should
  generate this table data-driven (name → integer value) rather than
  hand-enumerate every entry in this spec; see §5.8(k).

### 2.4 Events

**Module-level:** none. `node:crypto` itself is not an `EventEmitter`.

**Class-level:**
- `Cipheriv` / `Decipheriv` — inherit all `stream.Transform`/`Duplex` events:
  `'data'`, `'end'`, `'error'`, `'close'`, `'drain'`, `'finish'`, `'pipe'`,
  `'unpipe'`.
- `Sign` / `Verify` — inherit `stream.Writable` events: `'finish'`, `'error'`,
  `'close'`, `'drain'`, `'pipe'`, `'unpipe'`.
- No other class in this module emits events.

## 3. Types & option objects

```typescript
type Encoding = 'utf8' | 'utf-8' | 'ascii' | 'latin1' | 'binary' | 'base64'
  | 'base64url' | 'hex' | 'ucs2' | 'ucs-2' | 'utf16le' | 'utf-16le';

type BinaryLike = string | Buffer | NodeJS.TypedArray | DataView | ArrayBuffer;
type KeyLike = string | Buffer | KeyObject;

interface HashOptions {
  outputLength?: number; // for extendable-output functions (SHAKE128/256)
}

interface CipherCCMOptions extends stream.TransformOptions {
  authTagLength: number; // required for CCM
}
interface CipherGCMOptions extends stream.TransformOptions {
  authTagLength?: number; // default 16
}
interface CipherOCBOptions extends stream.TransformOptions {
  authTagLength: number;
}
interface CipherInfo {
  name: string;
  nid: number;
  blockSize?: number;
  ivLength?: number;
  keyLength: number;
  mode?: string;
}

interface RsaPublicKey {
  key: KeyLike;
  padding?: number;       // crypto.constants.RSA_*_PADDING
  oaepHash?: string;
  oaepLabel?: BinaryLike;
  encoding?: Encoding;
}
interface RsaPrivateKey extends RsaPublicKey {
  passphrase?: string | Buffer;
}

interface PrivateKeyInput {
  key: string | Buffer;
  format?: 'pem' | 'der' | 'jwk';
  type?: 'pkcs1' | 'pkcs8' | 'sec1';
  passphrase?: string | Buffer;
  encoding?: Encoding;
}
interface PublicKeyInput {
  key: string | Buffer;
  format?: 'pem' | 'der' | 'jwk';
  type?: 'pkcs1' | 'spki';
  encoding?: Encoding;
}

interface KeyExportOptions {
  type?: 'pkcs1' | 'pkcs8' | 'spki' | 'sec1';
  format?: 'pem' | 'der' | 'jwk';
  cipher?: string;               // private key only, requires passphrase
  passphrase?: string | Buffer;  // private key only
}

interface JsonWebKey {
  kty?: string; crv?: string; x?: string; y?: string; d?: string;
  n?: string; e?: string; k?: string; alg?: string;
  key_ops?: string[]; ext?: boolean; use?: string;
  [prop: string]: unknown;
}

interface AsymmetricKeyDetails {
  modulusLength?: number;          // RSA/DSA
  publicExponent?: bigint;         // RSA
  hashAlgorithm?: string;          // RSA-PSS
  mgf1HashAlgorithm?: string;      // RSA-PSS
  saltLength?: number;             // RSA-PSS
  divisorLength?: number;          // DSA/DH
  namedCurve?: string;             // EC
}

type KeyPairType = 'rsa' | 'rsa-pss' | 'dsa' | 'ec' | 'ed25519' | 'ed448'
  | 'x25519' | 'x448' | 'dh';

interface GenerateKeyPairOptionsBase {
  publicKeyEncoding?: { type: 'spki' | 'pkcs1', format: 'pem' | 'der' };
  privateKeyEncoding?: {
    type: 'pkcs8' | 'pkcs1' | 'sec1';
    format: 'pem' | 'der';
    cipher?: string;
    passphrase?: string | Buffer;
  };
}
interface RsaKeyPairOptions extends GenerateKeyPairOptionsBase {
  modulusLength: number;           // >= 512, recommend >= 2048
  publicExponent?: number;         // default 0x10001
}
interface RsaPssKeyPairOptions extends RsaKeyPairOptions {
  hashAlgorithm?: string;
  mgf1HashAlgorithm?: string;
  saltLength?: number;
}
interface DsaKeyPairOptions extends GenerateKeyPairOptionsBase {
  modulusLength: number;           // 1024 | 2048 | 3072
  divisorLength?: number;
}
interface EcKeyPairOptions extends GenerateKeyPairOptionsBase {
  namedCurve: string;              // e.g. 'prime256v1' | 'secp384r1' | 'secp521r1'
  paramEncoding?: 'named' | 'explicit'; // default 'named'
}
interface EdKeyPairOptions extends GenerateKeyPairOptionsBase {} // Ed25519/Ed448/X25519/X448 — no extra fields
interface DhKeyPairOptions extends GenerateKeyPairOptionsBase {
  primeLength?: number;
  prime?: Buffer;
  generator?: number;              // default 2
  group?: string;
}
type GenerateKeyPairOptions = RsaKeyPairOptions | RsaPssKeyPairOptions
  | DsaKeyPairOptions | EcKeyPairOptions | EdKeyPairOptions | DhKeyPairOptions;

interface ScryptOptions {
  cost?: number;    // N, default 16384 — CPU/memory cost, must be power of 2
  blockSize?: number; // r, default 8
  parallelization?: number; // p, default 1
  N?: number; r?: number; p?: number; // legacy aliases accepted
  maxmem?: number;  // default 32 * 1024 * 1024
}

interface RandomUUIDOptions {
  disableEntropyCache?: boolean; // default false
}

interface CheckPrimeOptions {
  checks?: number; // default 0 (auto) — number of Miller-Rabin rounds
}
interface GeneratePrimeOptions {
  add?: number | bigint;
  rem?: number | bigint;
  safe?: boolean;    // default false — generate a safe prime (p = 2q+1)
  bigint?: boolean;  // default false — return bigint instead of ArrayBuffer
}

interface Argon2Options {
  message: BinaryLike;
  nonce: BinaryLike;
  parallelism: number;
  tagLength: number;
  memory: number;      // KiB
  passes: number;
  version?: number;    // default 0x13
  ad?: BinaryLike;     // associated data
  secret?: BinaryLike;
}

interface SignPrivateKeyInput extends PrivateKeyInput {
  padding?: number;
  saltLength?: number;
  dsaEncoding?: 'der' | 'ieee-p1363';
}
interface VerifyPublicKeyInput extends PublicKeyInput {
  padding?: number;
  saltLength?: number;
  dsaEncoding?: 'der' | 'ieee-p1363';
}
interface SignKeyObjectInput {
  key: KeyObject; padding?: number; saltLength?: number;
  dsaEncoding?: 'der' | 'ieee-p1363';
}
interface VerifyKeyObjectInput extends SignKeyObjectInput {}

interface X509CheckOptions {
  subject?: 'always' | 'never'; // default 'always'; checkHost also accepts wildcards, partial wildcards
  wildcards?: boolean;          // checkHost only, default true
  partialWildcards?: boolean;   // checkHost only, default true
  multiLabelWildcards?: boolean;// checkHost only, default false
  singleLabelSubdomains?: boolean; // checkHost only, default false
}

// --- Web Crypto algorithm dictionaries ---
type KeyFormat = 'raw' | 'pkcs8' | 'spki' | 'jwk' | 'raw-secret' | 'raw-public' | 'raw-seed';
type KeyUsage = 'encrypt' | 'decrypt' | 'sign' | 'verify' | 'deriveKey'
  | 'deriveBits' | 'wrapKey' | 'unwrapKey' | 'encapsulateBits'
  | 'encapsulateKey' | 'decapsulateBits' | 'decapsulateKey';

interface RsaHashedKeyGenParams { name: 'RSA-OAEP' | 'RSA-PSS' | 'RSASSA-PKCS1-v1_5'; modulusLength: number; publicExponent: Uint8Array; hash: string; }
interface EcKeyGenParams { name: 'ECDSA' | 'ECDH'; namedCurve: 'P-256' | 'P-384' | 'P-521'; }
interface HmacKeyGenParams { name: 'HMAC'; hash: string; length?: number; }
interface AesKeyGenParams { name: 'AES-CBC' | 'AES-CTR' | 'AES-GCM' | 'AES-KW' | 'AES-OCB'; length: 128 | 192 | 256; }

interface RsaOaepParams { name: 'RSA-OAEP'; label?: BufferSource; }
interface AesCtrParams { name: 'AES-CTR'; counter: BufferSource; length: number; }
interface AesCbcParams { name: 'AES-CBC'; iv: BufferSource; }
interface AesGcmParams { name: 'AES-GCM'; iv: BufferSource; additionalData?: BufferSource; tagLength?: number; }
interface EcdsaParams { name: 'ECDSA'; hash: string; }
interface RsaPssParams { name: 'RSA-PSS'; saltLength: number; }
interface EcdhKeyDeriveParams { name: 'ECDH' | 'X25519' | 'X448'; public: CryptoKey; }
interface HkdfParams { name: 'HKDF'; hash: string; salt: BufferSource; info: BufferSource; }
interface Pbkdf2Params { name: 'PBKDF2'; hash: string; salt: BufferSource; iterations: number; }

interface CryptoKeyPair { publicKey: CryptoKey; privateKey: CryptoKey; }
```

## 4. Node semantics & edge cases

- **Encoding defaults.** When no `inputEncoding` is given and the input is a
  `string`, Node treats it as UTF-8. When no `outputEncoding`/output arg is
  given, methods return a `Buffer`; passing an encoding name coerces to
  `string`. Legacy encodings `'latin1'`/`'binary'` are aliases (byte-for-byte,
  one char per byte) and remain supported (no removal planned).
- **GCM/CCM/OCB auth tag.** `authTagLength` must be supplied to
  `createDecipheriv` explicitly for CCM/OCB; for GCM the default is 16 bytes.
  Calling `cipher.getAuthTag()` before `final()` throws
  `ERR_CRYPTO_INVALID_STATE`. `decipher.setAuthTag()` must be called before
  `update()`/`final()` for CCM, and may be called any time before `final()`
  for GCM.
- **`createCipher`/`createDecipher` (no-IV legacy) are REMOVED**, not merely
  deprecated — calling them throws (or is `undefined`) in modern Node
  (**DEP0106**, historically deprecated since v10, function removed in a
  later major). RTS must not implement them; only the `*iv` factories exist.
- **`pbkdf2`/`pbkdf2Sync` require an explicit `digest`** — omitting it used to
  default to `'sha1'` with a warning (**DEP0077**-class deprecation; verify
  exact code) and is now a hard `TypeError`.
- **`crypto.fips` property is deprecated** in favor of `getFips()`/`setFips()`
  (verify exact DEP code — commonly cited as **DEP0182**/**DEP0090**
  depending on Node version; do not hardcode a number without re-checking the
  changelog at implementation time).
- **`scryptSync`/`scrypt` memory ceiling.** Required memory ≈ `128 * N * r`
  bytes; if this exceeds `maxmem` (default 32 MiB) Node throws
  `ERR_CRYPTO_INVALID_SCRYPT_PARAMS`. `N` must be a power of two > 1.
- **`timingSafeEqual`** requires both buffers to have identical
  `byteLength`, else throws `ERR_CRYPTO_TIMING_SAFE_EQUAL_LENGTH` — comparing
  different-length buffers is a programmer error, not a "return false".
- **`randomInt` range.** `max - min` must be `<= 2**48`; otherwise throws
  `ERR_OUT_OF_RANGE`. Uses rejection sampling internally for unbiased output
  (RTS must not naively `% range`, which is biased).
- **`crypto.getRandomValues` size cap.** Maximum 65 536 bytes per call
  (`quota_bytes_per_request` — mirrors the WebCrypto spec's limit); exceeding
  it throws (Web Crypto style error, `ERR_CRYPTO_INVALID_MESSAGELEN`-class or
  a `DOMException`-shaped error; verify exact Node error object shape).
  Non-integer TypedArrays (`Float32Array`/`Float64Array`) are rejected with
  `TypeError`.
- **FIPS mode.** Only meaningful if Node was built against an
  OpenSSL 3.x FIPS provider; `setFips(true)` throws
  `ERR_CRYPTO_FIPS_UNAVAILABLE` on a non-FIPS build, and throws
  `ERR_CRYPTO_FIPS_FORCED` if the process was started with `--force-fips`
  (immutable for the process lifetime).
- **`secureHeapUsed()`** requires the process to have been started with
  `--secure-heap=<n>`; **not supported on Windows** (OpenSSL secure heap is a
  POSIX `mmap`/`mlock` facility) — throws or returns zeroed stats on Windows.
  This is the one genuine Windows/POSIX divergence in this module (unlike
  `fs`, crypto itself has no path-separator-style platform split).
- **`setEngine()`** depends on OpenSSL's legacy ENGINE API; many modern
  OpenSSL builds (3.x, provider-based) have reduced/no ENGINE support. Treat
  as best-effort / may throw `ERR_CRYPTO_ENGINE_UNKNOWN` even when the engine
  name looks valid, depending on the OpenSSL build.
- **Ordering / statefulness.** `Hash`/`Hmac`/`Sign`/`Verify`/`Cipheriv`/
  `Decipheriv` are **stateful, single-use** objects: calling `digest()`/
  `sign()`/`verify()`/`final()` a second time throws
  `ERR_CRYPTO_INVALID_STATE` (Node: `"Digest already called"` and friends).
  `hash.copy()` exists specifically so a caller can snapshot mid-stream state
  without this restriction.
- **Backpressure.** `Cipheriv`/`Decipheriv` are `Transform` streams; when used
  via `.pipe()`, standard stream backpressure (`'drain'` on writable side)
  applies. The synchronous `update()`/`final()` calling convention (not
  piped) has no backpressure concern — it's fully synchronous per call.
- **KDF import constraint (Web Crypto).** `importKey`/`deriveKey` for
  KDF-only algorithms (`PBKDF2`, `HKDF`, `Argon2*`) require `extractable ===
  false`; passing `true` throws `SyntaxError`-class error.
- **Deprecated/removed proprietary formats.** `'node.keyObject'`, `'NODE-DSA'`,
  `'NODE-DH'`, `'NODE-SCRYPT'` (proprietary JWK-adjacent extensions) were
  removed; do not implement.
- **Error taxonomy to reproduce:** `ERR_CRYPTO_INVALID_DIGEST`,
  `ERR_CRYPTO_INVALID_KEYLEN`, `ERR_CRYPTO_INVALID_KEYPAIR`,
  `ERR_CRYPTO_INVALID_SCRYPT_PARAMS`, `ERR_CRYPTO_INVALID_STATE`,
  `ERR_CRYPTO_TIMING_SAFE_EQUAL_LENGTH`, `ERR_CRYPTO_UNSUPPORTED_OPERATION`,
  `ERR_CRYPTO_ECDH_INVALID_PUBLIC_KEY`, `ERR_CRYPTO_ENGINE_UNKNOWN`,
  `ERR_CRYPTO_FIPS_UNAVAILABLE`, `ERR_CRYPTO_FIPS_FORCED`,
  `ERR_CRYPTO_INCOMPATIBLE_KEY`, `ERR_CRYPTO_INCOMPATIBLE_KEY_OPTIONS`,
  `ERR_CRYPTO_SIGN_KEY_REQUIRED`, `ERR_CRYPTO_JWK_UNSUPPORTED_CURVE`,
  `ERR_CRYPTO_JWK_UNSUPPORTED_KEY_TYPE`, `ERR_OSSL_*` (raw OpenSSL error
  passthrough with reason strings), `ERR_OUT_OF_RANGE`.
- **Security note.** `Math.random()`-derived values must never back any
  function in this module; every "random" primitive here (`randomBytes`,
  `randomInt`, `randomUUID`, `getRandomValues`, key generation, IV/salt/nonce
  generation) must be sourced from an OS CSPRNG, never the engine's
  non-cryptographic `math.random` xorshift64 generator described in the RTS
  runtime docs.

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node` owns this module end-to-end; it must **not** reuse rts-std's
existing `crypto` namespace (inline SHA-256 + CSPRNG under
`rts-runtime/src/namespaces/crypto/`), which is scheduled for deletion from
rts-std per the surface-redesign decision. `rts-node`'s crypto vendors its
own dependency set (pure-Rust, `RustCrypto`-family crates, matching the
project's existing "no OpenSSL/schannel dependency" stance already
established for `rts-std`'s `tls` namespace):

| Area | Rust crate(s) | Notes |
|---|---|---|
| SHA-1/224/256/384/512, SHA3, SHAKE | `sha1`, `sha2`, `sha3` | one-shot + streaming `Digest` trait fits `Hash.update/digest` directly |
| HMAC | `hmac` (generic over any `Digest`) | |
| AES-CBC/CTR/GCM/KW | `aes`, `cbc`, `ctr`, `aes-gcm`, `aes-kw` | AES-OCB has no mainstream pure-Rust crate yet — flag as a gap (§7) |
| ChaCha20-Poly1305 | `chacha20poly1305` | |
| RSA (encrypt/decrypt/sign/verify/keygen, OAEP/PSS/PKCS1v1.5) | `rsa` | slower keygen than OpenSSL; acceptable for P1 |
| EC (P-256/P-384/P-521, ECDSA, ECDH) | `p256`, `p384`, `p521` (`elliptic-curve`/`ecdsa` traits) | |
| Ed25519/X25519 | `ed25519-dalek`, `x25519-dalek` | |
| Ed448/X448 | `ed448-goldilocks` | lower-maturity crate; flag as gap if missing features |
| DSA / classic (finite-field) DH | `dsa`, `num-bigint-dig` or hand-rolled modexp over `num-bigint` | legacy, low priority (§5.8) |
| PBKDF2 | `pbkdf2` | |
| scrypt | `scrypt` | crate already exposes `Params { log_n, r, p }` matching Node's `N/r/p` |
| HKDF | `hkdf` | |
| Argon2 | `argon2` | experimental tier, matches Node's own "experimental" tag |
| CSPRNG | `getrandom` (wraps `BCryptGenRandom` on Windows / `getrandom(2)`/`/dev/urandom` on Unix) | **own copy in rts-node**, independent of rts-std's CSPRNG code path, per the no-rts-std-dependency decision |
| UUID v4 | hand-rolled 16 random bytes + RFC 4122 version/variant bit twiddling (avoids pulling the `uuid` crate for one function), or the `uuid` crate if a dependency is acceptable | |
| DER/PEM/PKCS8/SPKI/SEC1 encoding | `der`, `pkcs8`, `spki`, `sec1`, `pem-rfc7468` (RustCrypto `formats` family) | needed by every `KeyObject.export`/`createPrivateKey`/`generateKeyPair*` path |
| JWK | `elliptic_curve::JwkEcKey`/hand-rolled base64url JSON per key type | exported as a JSON string from the native layer (see §5.5) |
| X.509 parsing/inspection | `x509-parser` (or `x509-cert` + `der`) | read-only inspection only — Node's `X509Certificate` never *issues* certs, only reads them |
| Timing-safe compare | hand-rolled constant-time XOR-fold (no crate needed, a handful of lines) | |
| ML-KEM / ML-DSA / KMAC / TurboSHAKE / KangarooTwelve | lowest priority; candidate crates (`ml-kem`, `ml-dsa`, `tiny-keccak`/`sha3` extensions) evaluated at implementation time | experimental in Node itself — defer, see §7 |

### 5.2 ABI surface

Symbol convention: `__RTS_FN_NODE_CRYPTO_<NAME>`, one typed `extern "C"` per
primitive operation — no `__rts_call_dispatch`, matching the project-wide ABI
doctrine.

**Handle model.** Rich stateful objects (`Hash`, `Hmac`, `Cipheriv`/
`Decipheriv`, `Sign`, `Verify`, `DiffieHellman`/`DiffieHellmanGroup`, `ECDH`,
`KeyObject`, `X509Certificate`, `CryptoKey`) are opaque `u64` handles. Because
`rts-node` does not share `rts-engine`'s GC-managed `HandleTable` variant enum
(that enum — `Entry::String/BigFixed/Buffer/.../Free` — lives in
`rts-engine`/`rts-runtime` and is not meant to grow node-specific arms per the
primordial-vs-registry doctrine's spirit of not polluting shared engine
plumbing with backend-specific cases), `rts-node` maintains its **own**
sharded slab-style handle table (same 16-bit-generation + 48-bit-slot
encoding style as `rts-engine::abi::handles`, but a private table, analogous
to how `rts-napi` keeps its own external-reference table rather than
extending the shared `Entry` enum). Buffers/byte spans reuse `AbiType::StrPtr`
(2 slots: ptr + len) for raw bytes — it is not restricted to UTF-8 in
practice elsewhere in the codebase, and this spec follows that convention
explicitly (documented here to avoid confusion, since the type's doc-name
says "UTF-8").

Representative symbol table (not exhaustive — one row per primitive
operation category; each Node method in §2 maps 1:1 to one row here plus a
`.ts` shim call):

| Symbol | Args (`AbiType`) | Returns | Maps to |
|---|---|---|---|
| `__RTS_FN_NODE_CRYPTO_RANDOM_BYTES` | `[I64]` (size) | `Handle` (Buffer) | `randomBytes` |
| `__RTS_FN_NODE_CRYPTO_RANDOM_FILL` | `[Handle, I64, I64]` (buf, offset, size) | `Void` | `randomFillSync` |
| `__RTS_FN_NODE_CRYPTO_RANDOM_INT` | `[I64, I64]` (min, max) | `I64` | `randomInt` |
| `__RTS_FN_NODE_CRYPTO_RANDOM_UUID` | `[Bool]` (disableEntropyCache) | `StrPtr` | `randomUUID` |
| `__RTS_FN_NODE_CRYPTO_TIMING_SAFE_EQUAL` | `[StrPtr, StrPtr]` | `Bool` | `timingSafeEqual` |
| `__RTS_FN_NODE_CRYPTO_HASH_NEW` | `[StrPtr]` (algorithm) | `Handle` | `createHash` |
| `__RTS_FN_NODE_CRYPTO_HASH_UPDATE` | `[Handle, StrPtr]` | `Void` | `hash.update` |
| `__RTS_FN_NODE_CRYPTO_HASH_DIGEST` | `[Handle, StrPtr]` (out encoding) | `StrPtr` | `hash.digest` |
| `__RTS_FN_NODE_CRYPTO_HASH_COPY` | `[Handle]` | `Handle` | `hash.copy` |
| `__RTS_FN_NODE_CRYPTO_HASH_ONESHOT` | `[StrPtr, StrPtr, StrPtr]` (algo, data, outEnc) | `StrPtr` | `crypto.hash` |
| `__RTS_FN_NODE_CRYPTO_HMAC_NEW` | `[StrPtr, StrPtr]` (algorithm, key bytes) | `Handle` | `createHmac` |
| `__RTS_FN_NODE_CRYPTO_HMAC_UPDATE` | `[Handle, StrPtr]` | `Void` | `hmac.update` |
| `__RTS_FN_NODE_CRYPTO_HMAC_DIGEST` | `[Handle, StrPtr]` | `StrPtr` | `hmac.digest` |
| `__RTS_FN_NODE_CRYPTO_CIPHERIV_NEW` | `[StrPtr, StrPtr, StrPtr, StrPtr]` (algo, key, iv, opts-json) | `Handle` | `createCipheriv` |
| `__RTS_FN_NODE_CRYPTO_CIPHERIV_UPDATE` | `[Handle, StrPtr]` | `StrPtr` | `cipher.update` |
| `__RTS_FN_NODE_CRYPTO_CIPHERIV_FINAL` | `[Handle]` | `StrPtr` | `cipher.final` |
| `__RTS_FN_NODE_CRYPTO_CIPHERIV_SET_AAD` | `[Handle, StrPtr, I64]` (buf, plaintextLength) | `Void` | `cipher.setAAD` |
| `__RTS_FN_NODE_CRYPTO_CIPHERIV_GET_AUTH_TAG` | `[Handle]` | `StrPtr` | `cipher.getAuthTag` |
| `__RTS_FN_NODE_CRYPTO_DECIPHERIV_SET_AUTH_TAG` | `[Handle, StrPtr]` | `Void` | `decipher.setAuthTag` |
| `__RTS_FN_NODE_CRYPTO_KEY_FROM_PEM` | `[StrPtr, StrPtr, StrPtr]` (pem, kind, passphrase) | `Handle` (KeyObject) | `createPrivateKey`/`createPublicKey` |
| `__RTS_FN_NODE_CRYPTO_KEY_SECRET` | `[StrPtr]` | `Handle` | `createSecretKey` |
| `__RTS_FN_NODE_CRYPTO_KEY_EXPORT` | `[Handle, StrPtr]` (options JSON) | `StrPtr` | `keyObject.export` (returns PEM/base64-DER/JSON-JWK string; caller `JSON.parse`s for JWK) |
| `__RTS_FN_NODE_CRYPTO_KEY_TYPE` | `[Handle]` | `StrPtr` | `keyObject.type` |
| `__RTS_FN_NODE_CRYPTO_KEY_ASYMMETRIC_TYPE` | `[Handle]` | `StrPtr` | `keyObject.asymmetricKeyType` |
| `__RTS_FN_NODE_CRYPTO_KEY_EQUALS` | `[Handle, Handle]` | `Bool` | `keyObject.equals` |
| `__RTS_FN_NODE_CRYPTO_GENERATE_KEYPAIR` | `[StrPtr, StrPtr]` (type, options JSON) | `Handle` (packed pair — see below) | `generateKeyPairSync` |
| `__RTS_FN_NODE_CRYPTO_SIGN_NEW` | `[StrPtr]` (algorithm) | `Handle` | `createSign` |
| `__RTS_FN_NODE_CRYPTO_SIGN_UPDATE` | `[Handle, StrPtr]` | `Void` | `sign.update` |
| `__RTS_FN_NODE_CRYPTO_SIGN_FINAL` | `[Handle, Handle, StrPtr]` (sign handle, key handle, options JSON) | `StrPtr` | `sign.sign` |
| `__RTS_FN_NODE_CRYPTO_VERIFY_FINAL` | `[Handle, Handle, StrPtr, StrPtr]` | `Bool` | `verify.verify` |
| `__RTS_FN_NODE_CRYPTO_DH_NEW` | `[I64, I64]` (primeLength, generator) | `Handle` | `createDiffieHellman` |
| `__RTS_FN_NODE_CRYPTO_DH_COMPUTE_SECRET` | `[Handle, StrPtr]` | `StrPtr` | `diffieHellman.computeSecret` |
| `__RTS_FN_NODE_CRYPTO_ECDH_NEW` | `[StrPtr]` (curveName) | `Handle` | `createECDH` |
| `__RTS_FN_NODE_CRYPTO_PBKDF2` | `[StrPtr, StrPtr, I64, I64, StrPtr]` | `StrPtr` | `pbkdf2Sync` |
| `__RTS_FN_NODE_CRYPTO_SCRYPT` | `[StrPtr, StrPtr, I64, StrPtr]` (opts JSON) | `StrPtr` | `scryptSync` |
| `__RTS_FN_NODE_CRYPTO_HKDF` | `[StrPtr, StrPtr, StrPtr, StrPtr, I64]` | `StrPtr` | `hkdfSync` |
| `__RTS_FN_NODE_CRYPTO_CHECK_PRIME` | `[StrPtr, I64]` (candidate bytes, checks) | `Bool` | `checkPrimeSync` |
| `__RTS_FN_NODE_CRYPTO_GENERATE_PRIME` | `[I64, StrPtr]` (size, options JSON) | `StrPtr` | `generatePrimeSync` |
| `__RTS_FN_NODE_CRYPTO_X509_NEW` | `[StrPtr]` (DER/PEM bytes) | `Handle` | `new X509Certificate()` |
| `__RTS_FN_NODE_CRYPTO_X509_CHECK_HOST` | `[Handle, StrPtr, StrPtr]` | `StrPtr` | `x509.checkHost` |
| `__RTS_FN_NODE_CRYPTO_SUBTLE_DIGEST` | `[StrPtr, StrPtr]` | `Handle` (ArrayBuffer) | `subtle.digest` (wrapped in a Promise by the `.ts` shim) |

Option objects that don't cleanly decompose into scalar `AbiType` args
(`GenerateKeyPairOptions`, `CipherCCMOptions`, `GeneratePrimeOptions`,
`X509CheckOptions`, JWK bodies, algorithm dictionaries for `SubtleCrypto`)
cross the ABI as a single JSON-encoded `StrPtr` — the native side parses it
with `serde_json` (an internal dependency, never exposed as a "high-level API
in Rust", purely a marshalling detail) rather than adding dozens of
positional scalar parameters per symbol. `generateKeyPair` returning **two**
independent handles is packed as `(pub_handle << 32) | priv_handle` in one
`u64`/`Handle` slot, unpacked by the `.ts` shim into
`{ publicKey, privateKey }` — RTS avoids inventing a two-`Handle`-return ABI
convention for this one call site.

**`.ts` shim vs native extern split:**
- **Native externs**: every primitive listed above — digest/cipher/KDF/sign/
  verify/DH/ECDH computation, CSPRNG draws, DER/PEM parse+encode,
  handle lifecycle (`_NEW`/`_FREE`), timing-safe compare, primality tests.
- **`.ts` shim** (ships inside `rts-node`'s own bundled stdlib, not
  `rts-shared`): the `Certificate`, `Cipheriv`, `Decipheriv`, `DiffieHellman`,
  `DiffieHellmanGroup`, `ECDH`, `Hash`, `Hmac`, `KeyObject`, `Sign`, `Verify`,
  `X509Certificate` classes (constructor argument normalization, encoding
  defaults, chainable `update()` returning `this`, JSON option
  stringification before crossing the ABI, JWK `JSON.parse` on export); the
  `Crypto`/`SubtleCrypto`/`CryptoKey` Web Crypto layer (Promise wrapping,
  algorithm-name canonicalization, `KeyUsage` validation); every module-level
  convenience function's default-argument handling (e.g. `randomBytes(size)`
  with no callback vs. with callback dispatches to the same native call
  either directly or through the async bridge, §5.3).

### 5.3 Async model

| Area | Sync | Callback | Promise |
|---|---|---|---|
| `Hash`/`Hmac`/`Sign`/`Verify` update/digest/final, `Cipheriv`/`Decipheriv` update/final/setAAD/setAuthTag/getAuthTag | ✅ always sync — no async variant exists in Node either | — | — |
| `DiffieHellman`/`DiffieHellmanGroup`/`ECDH` all methods | ✅ always sync | — | — |
| `randomBytes` | ✅ (no callback) | ✅ (callback given) → offload | — |
| `randomFill` | — (no sync-without-callback form; use `randomFillSync`) | ✅ | — |
| `randomFillSync`, `randomInt` (no callback), `randomUUID`, `getRandomValues`, `timingSafeEqual` | ✅ | `randomInt` also has a callback form | — |
| `pbkdf2`/`scrypt`/`hkdf` | `*Sync` variants | ✅ non-`Sync` variants → offload (scrypt/pbkdf2 are the canonical CPU-heavy case) | — |
| `argon2` | `argon2Sync` | ✅ `argon2` → offload | — |
| `generateKeyPair`/`generateKey` | `*Sync` variants | ✅ non-`Sync` → offload (RSA/DSA/DH keygen is the slow case) | — |
| `checkPrime`/`generatePrime` | `*Sync` variants | ✅ non-`Sync` → offload (`generatePrime` for large sizes is slow) | — |
| `sign`/`verify` (one-shot) | ✅ (no callback) | ✅ (callback given) → offload | — |
| `diffieHellman()` | ✅ (no callback) | ✅ (callback given) → offload | — |
| `encapsulate`/`decapsulate` | `decapsulate` sync form exists | ✅ callback forms → offload | `encapsulate` also promise-shaped per fetched signature — clarify against changelog at impl time |
| **All `SubtleCrypto` methods** | — | — | ✅ **mandatory** — every method returns a `Promise`, even trivially fast ones (`digest` on a short input), per the Web Crypto spec |

**Offload mechanism.** Every "→ offload" cell uses the same pattern as the
rest of RTS's async surface (`docs/specs/async-promise-function.md`): the
callback-style call schedules the blocking native computation on the shared
tokio runtime via `spawn_blocking`, then invokes the JS callback (a
`Function` handle) with `(err, result)` on completion, posted back through
the event loop — this is a plain Node-style callback, not a `Promise`, so it
does **not** go through `promise.create`. `SubtleCrypto` methods, by
contrast, are Promise-native: each wraps its (possibly cheap, possibly
`spawn_blocking`-offloaded) native call in `promise.create(fn, args)` exactly
like a rewritten `async function`, so `await crypto.subtle.digest(...)` and
`.then()` chains work uniformly regardless of whether the underlying op was
cheap enough to run inline or needed the thread pool. See §5.7 for why this
needs infra currently living in `rts-std`.

### 5.4 Multithread / worker interaction

- **Per-call-site handles are thread-owned, not shared.** `Hash`/`Hmac`/
  `Cipheriv`/`Decipheriv`/`Sign`/`Verify`/`DiffieHellman`/`ECDH` in-progress
  streaming state must never be silently shared across an RTS worker
  boundary — this matches Node itself, which throws `DataCloneError` if you
  try to `postMessage` a `Hash` mid-stream. Under the RTS threading model
  (`docs/specs/rts-threading-model.md`), these handles live in a
  **per-thread region**; a handle table lookup from another thread must fail
  fast (owning-thread-id tag check) rather than silently mutate cross-thread
  state.
- **`KeyObject`/`CryptoKey` ARE structured-clone-safe in Node** (transferring
  key material across `worker_threads` is a supported, common pattern for
  sharing a loaded private key with worker pools). Map this onto the
  threading model's **shared-heap promotion on publication**: the first time
  a `KeyObject`/`CryptoKey` handle crosses a `channel`/`postMessage`-style
  boundary, its underlying key-material buffer is promoted from the
  originating thread's region into the shared heap; the receiving thread gets
  a plain handle (slot index) copy, not a deep clone of key bytes — mirrors
  how `SharedArrayBuffer` is described as shared-heap memory in the threading
  model.
- **Process-wide singletons.** FIPS mode (`getFips`/`setFips`), the loaded
  OpenSSL-style engine (`setEngine`, legacy), and `secureHeapUsed()`'s
  underlying secure-heap allocator are genuinely **process-global**, not
  per-thread — implement as one `OnceLock<Mutex<...>>`-style singleton
  (matches the "no central state system, each namespace owns its own via
  `Arc<Mutex<T>>`/`OnceLock`" runtime convention), explicitly **not**
  duplicated per RTS thread/region.
- **CSPRNG state.** Unlike the FIPS/engine singletons, the random-byte source
  should be **per-thread** (a thread-local CSPRNG handle re-seeded from OS
  entropy on first use per thread), so that concurrent `randomBytes`/
  `getRandomValues` calls from worker threads never contend on one mutex —
  this mirrors the existing `HandleTable`'s 32-shard round-robin design
  philosophy (avoid a single lock becoming a scalability wall under
  `worker_threads`/`thread.spawn` fan-out).
- **`worker_threads` mapping (forward-looking, not this module's concern to
  implement, but the shape this module must be compatible with):** a Node
  `Worker` = an RTS thread/region; `MessagePort`/`postMessage` = a `channel`;
  the `KeyObject`/`CryptoKey` promotion rule above is what makes "send a
  loaded TLS certificate's private key to a worker pool" work correctly
  under that mapping.

### 5.5 Buffer / TypedArray interop

- Every `BinaryLike` parameter (`string | Buffer | TypedArray | DataView |
  ArrayBuffer`) is normalized by the `.ts` shim to a `(ptr, len)` byte span
  before crossing the ABI as `AbiType::StrPtr` — strings are UTF-8-encoded
  first (matching Node's default when no `inputEncoding` given); legacy
  encodings (`'latin1'`/`'hex'`/`'base64'`/etc.) are decoded to raw bytes in
  the `.ts` shim, **not** re-implemented natively per encoding (byte-decoding
  utilities for hex/base64 already need to exist elsewhere in `rts-node` for
  `Buffer`-adjacent surface; this module reuses those, it does not vendor its
  own hex/base64 codec a second time).
- Native functions that produce arbitrary-length output (digests, cipher
  ciphertext, KDF-derived keys, exported key material) allocate a
  GC-owned/handle-table-owned byte buffer and return it as a `Handle` (when
  the caller wants a `Buffer`/`Uint8Array`) or as `StrPtr` (when the output
  encoding requested — `'hex'`/`'base64'`/`'base64url'`/PEM text — is
  string-shaped); the `.ts` shim picks the right native entry point based on
  whether an `outputEncoding` was passed, matching Node's own overload
  behavior (`Buffer` vs `string` return type based on presence of an
  encoding argument).
- **`KeyObject.export({format:'jwk'})` and generic JWK import/export**: the
  native layer returns a single JSON-encoded `StrPtr` (never constructs a JS
  object graph in Rust, per "no high-level API in Rust — only raw
  primitives"); the `.ts` shim does `JSON.parse()` to produce the JWK object
  the caller sees. The same JSON-string convention is used for any other
  multi-field return shape that doesn't fit scalar `AbiType`s
  (`AsymmetricKeyDetails`, `CipherInfo`, `secureHeapUsed()`'s stats object).
- **`ArrayBuffer`/TypedArray results from `SubtleCrypto`** (spec-mandated
  `ArrayBuffer` return type, not `Buffer`) are constructed by the `.ts` shim
  by wrapping the same GC-owned byte handle the native call produced — no
  separate "ArrayBuffer vs Buffer" code path natively, only a different JS
  wrapper type at the shim layer (both are backed by the primordial
  ArrayBuffer/TypedArray memory model per the engine doctrine).
- **`getRandomValues(typedArray)` writes in place** (fills caller-supplied
  memory rather than allocating new); the native extern takes the caller's
  buffer pointer + length directly (`AbiType::StrPtr` used as an in/out
  parameter) and returns `Void`, with the `.ts` shim returning the same
  `typedArray` reference back to match the Web Crypto signature (`=> T`, not
  a new array).

### 5.6 Doctrine placement

`node:crypto` is unambiguously **non-primordial**: nothing in it has native
JS/TS syntax (no `Hash` literal, no cipher literal — contrast `RegExp`'s
`/re/` or `Error`'s `throw`/`catch` integration). The engine
(`crates/rts-codegen-new/`) must never hardcode `"crypto"`, `"Hash"`,
`"Cipheriv"`, `"KeyObject"`, or any other name from this module — it only
ever sees a fully-qualified member name like `node_crypto.createHash` and
resolves it through the **existing node-registry data path** already wired
in `crates/rts-node/src/lib.rs`:

```
import { createHash } from "node:crypto"
        │
        ▼ ns_prefix_for("node:crypto") -> "node_crypto"   (data lookup, NODE_SPECS)
        │
        ▼ node_lookup("node_crypto.createHash") -> &NodespaceMember { symbol: "__RTS_FN_NODE_CRYPTO_HASH_NEW", ... }
```

This is exactly the same generic mechanism `fs`/`path`/`os`/`process`/`util`
already use in the current (soon-to-be-rewritten) `rts-node` crate — adding
`crypto::SPEC` to `NODE_SPECS` in `lib.rs` is the entire "registration"
surface the engine needs; no codegen change is required to add this module,
by construction of the doctrine.

The native-extern / `.ts`-shim split is as described in §5.2/§5.3: raw
primitive operations + handle lifecycle are native `extern "C"` symbols
harvested into `NodespaceMember` rows; every JS-shaped class, every
default-argument/encoding/option-normalization concern, and the entire
Promise-wrapping layer for Web Crypto is `.ts` shipped inside `rts-node`
itself (not `rts-shared`, since `rts-node` is independent and does not share
`rts-shared`'s stdlib bundle either — only *primordial* `.ts` lives in
`rts-primitives`, and this module is not primordial, so its `.ts` lives in
`rts-node`'s own bundled stdlib directory, parallel to but separate from
`rts-shared/src/stdlib/*.ts`).

### 5.7 Shared-infra dependencies (FLAG)

- **Tokio runtime / `spawn_blocking`** (`async_rt.rs`'s shared multi-thread
  `OnceLock<Runtime>`) — needed for every "→ offload" cell in §5.3
  (`randomBytes`/`randomFill` callback forms, `pbkdf2`/`scrypt`/`hkdf`/
  `argon2` callback forms, `generateKeyPair`/`generateKey` callback forms,
  `checkPrime`/`generatePrime` callback forms, `sign`/`verify`/
  `diffieHellman` callback forms, `encapsulate`/`decapsulate`) **and** for
  every `SubtleCrypto` method (all Promise-mandatory). **Currently lives in
  `rts-std`** (`crates/rts-runtime/.../runtime/async_rt.rs`, per
  `.claude/rules/02-runtime.md`). Since `rts-node` must not depend on
  `rts-std`, this needs to be hoisted into a shared low crate both can reach
  — either promoted into `rts-engine` itself, or a new thin `rts-async-core`
  crate sitting beside `rts-primitives` in the dependency graph
  (`rts-engine ← rts-async-core ← {rts-std, rts-node}`).
- **Promise subsystem** (`promise.create`/settle, thread-local error slot,
  the `#437` design) — needed specifically for `SubtleCrypto`'s
  mandatory-Promise surface (every `subtle.*` call is
  `promise.create(native_fn, args)`-shaped). **Currently lives in `rts-std`**
  (`namespaces/promise`). Same hoisting requirement as the tokio runtime
  above — without it, `crypto.subtle.digest(...).then(...)` cannot be
  implemented from inside `rts-node` alone.
- **Event loop / microtask drain** — `SubtleCrypto` promises must resolve
  through the same microtask queue user `async`/`await` code drains;
  currently `rts-std`-owned infrastructure. Same hoisting requirement.
- **Callback-invocation bridge** (`Function`'s `invoke_n` trampoline used to
  call back into a JS callback with `(err, result)`) — needed for every
  Node-style-callback offload path in §5.3. `Function` is a **primordial**
  class, so per the crate-partition doctrine its implementation should
  already live in `rts-primitives` (engine-adjacent, not `rts-std`) — if,
  in practice, `invoke_n`/the callback trampoline still physically resides
  under `rts-std`'s `globals/function/ops.rs` at implementation time, **that
  is itself a pre-existing doctrine violation to fix first** (Function
  should not require an `rts-std` dependency), not something `node:crypto`
  should work around by duplicating a trampoline.
- **Crypto primitives themselves are explicitly NOT a shared-infra need** —
  by design (owner decision), `rts-node` vendors its own hash/cipher/KDF/
  keygen implementations (§5.1) rather than reusing `rts-std`'s existing
  inline SHA-256/CSPRNG `crypto` namespace, which is slated for deletion from
  `rts-std` once `rts-node`'s version lands. Listed here only to make the
  non-dependency explicit and avoid an implementer reaching for the
  easy-looking shortcut of `use`-ing `rts-std`'s crypto code.
- **TLS/rustls** — **not needed by this module.** Certificate *validation*
  chains and TLS session crypto are `node:tls`'s concern; `node:crypto`'s
  `X509Certificate` only *parses/inspects* a certificate buffer it's handed,
  it never opens a socket or validates a chain against a trust store. No
  flag here; revisit when specifying `node:tls`.

If none of the above get hoisted before this module is implemented, the
pragmatic fallback is: ship the **synchronous** surface first (everything in
§5.3's "Sync" column, which is the majority of the module — all of `Hash`/
`Hmac`/`Cipheriv`/`Decipheriv`/`Sign`/`Verify`/`DiffieHellman`/`ECDH`/
`KeyObject` needs zero async infra), and gate the callback/Promise surface
(`SubtleCrypto`, the `*Sync`-less variants) on the hoist landing — this is
the explicit, justified partial-scope path referenced in §5.8.

### 5.8 Implementation phases

a. **Handle-table skeleton.** Stand up `rts-node`'s own private sharded
   handle table (mirroring `rts-engine::abi::handles`' gen16+slot48 encoding,
   but a separate table — see §5.2) plus the `NodespaceSpec`/`crypto::SPEC`
   registration boilerplate in `lib.rs`, wired to zero real members yet (a
   smoke-test round-trip: allocate + free one dummy handle).
b. **Randomness, no key material.** `randomBytes`, `randomFillSync`,
   `randomInt`, `randomUUID`, `getRandomValues`, `timingSafeEqual` — smallest
   viable slice, purely sync, no KeyObject/Hash/Cipher state, immediately
   useful (`node:crypto`'s single most common use case in practice).
c. **One-shot + streaming hashing.** `createHash`/`hash.update`/`hash.digest`/
   `hash.copy`, `crypto.hash` one-shot, `getHashes()`.
d. **HMAC.** `createHmac`/`hmac.update`/`hmac.digest`.
e. **Symmetric ciphers, sync only.** `createCipheriv`/`createDecipheriv` +
   `update`/`final`/`setAutoPadding` for AES-CBC/CTR first (no AEAD tag
   complexity), then AES-GCM (`setAAD`/`getAuthTag`/`setAuthTag`), then
   AES-KW, then ChaCha20-Poly1305; `getCiphers()`/`getCipherInfo()`.
f. **KDFs, sync only.** `pbkdf2Sync`, `scryptSync`, `hkdfSync`.
g. **Key management (symmetric + RSA first).** `createSecretKey`,
   `createPrivateKey`/`createPublicKey`/`KeyObject` (export/equals/type/
   asymmetricKeyType/asymmetricKeyDetails/symmetricKeySize),
   `generateKeyPairSync`/`generateKeySync` for `'rsa'`, then `'rsa-pss'`.
h. **Key management, EC + modern curves.** `generateKeyPairSync` for `'ec'`
   (P-256/384/521), `'ed25519'`, `'x25519'`, `'ed448'`, `'x448'`;
   `getCurves()`.
i. **Signing.** `createSign`/`createVerify`/`sign.sign`/`verify.verify`, plus
   one-shot `crypto.sign`/`crypto.verify`, for RSA-PSS/RSASSA-PKCS1-v1_5/
   ECDSA/Ed25519 first (Ed448/DSA lower priority).
j. **DH / ECDH.** `DiffieHellman`/`DiffieHellmanGroup`/`createDiffieHellman`/
   `createDiffieHellmanGroup`/`getDiffieHellman`, `ECDH`/`createECDH`,
   `crypto.diffieHellman(options)` convenience, `generateKeyPairSync('dh')`,
   legacy DSA keygen (lowest priority in this phase).
k. **X.509 + legacy Certificate.** `X509Certificate` (parse + all read-only
   properties/checks), legacy `Certificate` (SPKAC) class; also where the
   data-driven `crypto.constants` table (§2.3) gets generated (a static
   name→value Rust table, not hand-maintained per this spec's individual
   bullets).
l. **Primes.** `checkPrimeSync`/`generatePrimeSync`.
m. **Web Crypto, sync-backed subset first.** `Crypto`/`globalThis.crypto`
   wiring (ambient global singleton — reuse the `singleton_instance_globals`/
   `gcell_classes` generic-shape mechanism the engine already has for other
   ambient globals, per the anti-hardcode doctrine, **not** a `"crypto"`
   name check), `crypto.getRandomValues`/`crypto.randomUUID`,
   `subtle.digest`/`subtle.generateKey`/`subtle.importKey`/`subtle.exportKey`/
   `subtle.sign`/`subtle.verify`/`subtle.encrypt`/`subtle.decrypt` layered
   over the natives from phases c–j, each wrapped in a resolved/immediate
   Promise if the async-infra hoist (§5.7) hasn't landed yet, or via
   `promise.create` once it has.
n. **Web Crypto, key derivation + wrapping.** `subtle.deriveBits`/
   `subtle.deriveKey` (ECDH/HKDF/PBKDF2), `subtle.wrapKey`/`subtle.unwrapKey`,
   `SubtleCrypto.supports`.
o. **Async/callback variants, module-wide.** Once §5.7's hoist lands: add the
   callback forms (`randomBytes(size, cb)`, `pbkdf2`, `scrypt`, `hkdf`,
   `generateKeyPair`, `checkPrime`, `generatePrime`, `sign`/`verify` with
   callback, `diffieHellman` with callback, `encapsulate`/`decapsulate`) —
   ship sync-first per the rest of this list, this phase retrofits async
   onto everything at once rather than per-function, since it's one
   mechanical wrapper pattern.
p. **Experimental / long-tail algorithms.** `argon2`/`argon2Sync`, ML-KEM
   (`encapsulate*`/`decapsulate*`), ML-DSA, KMAC128/256, TurboSHAKE/
   KangarooTwelve/SHA-3-family digest names, AES-OCB — lowest priority,
   explicitly allowed to lag Node's experimental-tier timeline; document any
   gap (missing crate maturity) rather than hand-rolling primitives that
   belong in a vetted crypto library.

## 6. Test plan

`.test.ts` fixtures under `tests/` (`rts:test` harness, per project
convention — pre-compute values at top level, avoid calling instance methods
inside `test()` closures per the GC-timing gotcha):

- **`crypto_random.test.ts`** — `randomBytes(32).length === 32`;
  `randomBytes` non-determinism (two calls differ); `randomFillSync` fills a
  `Uint8Array` in place and returns the same reference; `randomInt(1, 7)`
  stays in `[1,7)` across many iterations; `randomInt(6)` stays in `[0,6)`;
  `randomUUID()` matches the v4 UUID regex and version/variant nibbles;
  `getRandomValues` on `Uint8Array`/`Uint32Array`/`Int16Array` fills
  in-place and rejects `Float64Array` with `TypeError`; `getRandomValues`
  over 65536 bytes throws; `timingSafeEqual` true/false cases and a
  length-mismatch throw.
- **`crypto_hash.test.ts`** — `createHash('sha256').update('abc').digest('hex')`
  against a known test vector; multi-`update()` chaining equals one
  concatenated `update()`; `digest()` called twice throws
  `ERR_CRYPTO_INVALID_STATE`; `hash.copy()` produces independent continuable
  state (diverging updates after copy give different digests);
  `crypto.hash('sha256', 'abc')` one-shot matches the streaming result;
  SHA-1/SHA-384/SHA-512/SHA3-256 vectors; unknown algorithm name throws
  `ERR_CRYPTO_INVALID_DIGEST`; empty-string input digest.
- **`crypto_hmac.test.ts`** — RFC 4231 HMAC-SHA256 test vectors;
  `Hmac` with a `Buffer` key vs a `string` key (UTF-8) produce the same MAC
  when bytes match; wrong key length still works (HMAC pads/hashes per spec).
- **`crypto_cipher_aes.test.ts`** — AES-256-CBC round-trip (encrypt then
  decrypt recovers plaintext) with a known-answer vector; AES-256-GCM
  round-trip including `getAuthTag()`/`setAuthTag()`; tampering with
  ciphertext before `decipher.final()` throws (auth tag mismatch);
  `setAAD` mismatch between encrypt/decrypt fails GCM verification;
  AES-CTR round-trip; wrong IV length throws; `update()` called after
  `final()` throws; empty plaintext round-trip.
- **`crypto_cipher_chacha.test.ts`** — ChaCha20-Poly1305 round-trip +
  auth-tag tamper detection.
- **`crypto_kdf.test.ts`** — `pbkdf2Sync` against an RFC 6070 test vector;
  `scryptSync` against Node's own documented `N=16384,r=8,p=1` reference
  output for a fixed password/salt; `scryptSync` with `maxmem` too small
  throws `ERR_CRYPTO_INVALID_SCRYPT_PARAMS`; `hkdfSync` against an RFC 5869
  test vector; `pbkdf2Sync` with 0 iterations throws/1 iteration minimum
  enforced.
- **`crypto_keys_rsa.test.ts`** — `generateKeyPairSync('rsa', {modulusLength:
  2048, ...})` produces a usable pair (sign with private, verify with
  public); PEM and DER encodings round-trip through `createPrivateKey`/
  `createPublicKey`; `KeyObject.export({format:'jwk'})` for RSA produces
  `n`/`e`/`d` fields that re-import correctly; `keyObject.equals` true for
  the same key material loaded twice, false across different keys;
  encrypted private key export (`cipher`+`passphrase`) round-trips only with
  the correct passphrase.
- **`crypto_keys_ec.test.ts`** — EC P-256/P-384/P-521 keygen + sign/verify
  (ECDSA) round-trip; Ed25519 keygen + sign/verify; X25519 keygen +
  `diffieHellman()`/ECDH-style shared-secret agreement between two
  generated pairs producing the same shared secret on both sides.
- **`crypto_sign_verify.test.ts`** — `createSign`/`createVerify` streaming
  API vs one-shot `crypto.sign`/`crypto.verify` produce the same result;
  RSA-PSS with explicit `saltLength`; tampered data fails verify; tampered
  signature fails verify; wrong key (mismatched pair) fails verify without
  throwing (returns `false`).
- **`crypto_dh.test.ts`** — classic `DiffieHellman`: two sides
  `generateKeys()` then `computeSecret(otherPublicKey)` agree; `verifyError`
  is 0 for a well-formed group; `getDiffieHellman('modp14')` well-known
  group round-trip; `ECDH` class `computeSecret` agreement, `generateKeys`
  format `'compressed'` vs `'uncompressed'` differ in length as expected.
- **`crypto_x509.test.ts`** — parse a fixed test certificate buffer;
  `subject`/`issuer`/`validFrom`/`validTo`/`serialNumber`/`fingerprint256`
  match expected fixed values; `checkHost`/`checkEmail` positive and
  negative cases including a wildcard SAN; `toLegacyObject()` shape;
  self-signed cert `checkIssued(itself)` true.
- **`crypto_webcrypto_subtle.test.ts`** — `globalThis.crypto` exists without
  import; `crypto.subtle.digest('SHA-256', data)` resolves to an
  `ArrayBuffer` matching the `createHash` result for the same bytes;
  `generateKey`/`sign`/`verify` round-trip for HMAC and ECDSA; `importKey`/
  `exportKey('raw', ...)` round-trip for an AES-GCM key; `encrypt`/`decrypt`
  round-trip for AES-GCM via `subtle`; PBKDF2 `deriveBits` matches
  `pbkdf2Sync` with equivalent parameters; awaiting multiple `subtle.*`
  calls concurrently (`Promise.all`) resolves all correctly (exercises the
  Promise/event-loop integration, not just single-await).
- **`crypto_multithread.test.ts`** *(gated on `worker_threads`/threading
  model landing)* — a `KeyObject`/`CryptoKey` created on the main thread,
  sent to a worker via `channel`/`postMessage`, used to `sign`/`verify` on
  the worker thread; concurrent `randomBytes` calls from multiple threads
  produce non-colliding, independently-valid output (exercises the
  per-thread CSPRNG design from §5.4); an in-progress `Hash` handle is
  **not** usable from a second thread (expects a clone/ownership error, not
  silent corruption).
- **`crypto_errors.test.ts`** — grab-bag of the error taxonomy in §4:
  `timingSafeEqual` length mismatch, `scrypt` params too large for
  `maxmem`, `randomInt` range too large, unknown hash/cipher algorithm
  names, calling `digest()`/`sign()`/`final()` twice, `setFips(true)` on a
  non-FIPS build.

## 7. Open questions / deferrals

- **AES-OCB and Ed448/X448 crate maturity.** No first-party, widely-audited
  pure-Rust crate was confidently identified for AES-OCB at spec-writing
  time (RustCrypto's AEAD family covers GCM/CCM/ChaCha20-Poly1305 solidly;
  OCB is less common outside OpenSSL). Ed448/X448 crate options exist
  (`ed448-goldilocks`) but are lower-maturity than the Ed25519/X25519
  ecosystem. Defer both to phase p (§5.8) and re-evaluate crate landscape at
  implementation time rather than hand-rolling either primitive.
- **Exact DEP-code numbers.** §4 flags two DEP codes ("verify") where the
  WebFetch-sourced material and general knowledge did not fully agree on the
  precise `DEP0XXX` number for (a) the `crypto.fips` property deprecation and
  (b) the historical `pbkdf2` digest-required change. Confirm against the
  Node 25 changelog/`doc/api/deprecations.md` before hardcoding either
  number in an error message or doc comment.
- **`crypto.constants`' long tail.** The `SSL_OP_*`/`ENGINE_*`/version-number
  constant set is large (100+) and mostly consumed by `node:tls` rather than
  typical `node:crypto` user code. Deferred to be generated data-driven
  (name → OpenSSL-equivalent integer) at whatever point `node:tls` also
  needs a subset of the same table, rather than transcribing every value by
  hand in this module alone.
- **ML-KEM/ML-DSA/KMAC/TurboSHAKE/KangarooTwelve.** All explicitly
  experimental in Node itself (WICG-track proposals, OpenSSL ≥3.5
  dependency in upstream Node). Treat as fully deferred; do not block P1
  completion on them.
- **FIPS mode meaningfulness.** RTS's own crypto stack (RustCrypto-family
  crates, §5.1) is not FIPS 140-validated. `getFips()`/`setFips()` can be
  implemented as a state flag for API-compatibility, but actually being
  correct/certifiable FIPS mode is out of scope — document this gap loudly
  rather than silently returning `true` from `setFips(true)`.
- **`setEngine()` / legacy OpenSSL ENGINE API.** Given RTS's crypto isn't
  OpenSSL-backed at all (pure Rust, per the no-OpenSSL/no-schannel stance
  already established for `tls`), `setEngine()` has no real native operation
  to perform. Candidate resolution: implement as a documented no-op that
  throws `ERR_CRYPTO_ENGINE_UNKNOWN` for any engine name (matching Node's own
  behavior on a build without that engine compiled in) rather than silently
  succeeding — needs an explicit decision at implementation time, not
  assumed here.
- **Async-infra hoist timing (§5.7) is a hard blocker for `SubtleCrypto` and
  every callback-style function**, not just a nice-to-have — this spec
  assumes it lands before or alongside phases m–o (§5.8); if it slips,
  ship the sync subset (phases a–l) as a standalone, explicitly-partial
  release and track the rest as a known gap, per the project's
  regress-when-necessary-but-explicit discipline.
- **JWK coverage breadth.** RFC 7517 JWK has many edge fields
  (`key_ops`/`use`/`x5c`/`x5t`/curve-specific fields for less common curves).
  This spec covers the common RSA/EC/OKP JWK shapes; exotic JWK fields
  (certificate chains embedded in a JWK, etc.) are deferred until a concrete
  use case demands them.
- **`Certificate` (SPKAC) class real-world relevance.** SPKAC (`<keygen>`
  HTML element output) is effectively obsolete outside legacy browser forms.
  Confirm with the team whether P1 should implement it at all versus stubbing
  with `todo!()` and tracking as a follow-up — included in this spec for
  completeness per Node's own surface, not because it's expected to be
  high-value.
