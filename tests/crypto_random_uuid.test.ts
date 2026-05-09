// crypto.random_uuid + node:crypto.randomUUID — UUID v4 RFC 4122.

import { describe, test, expect } from "rts:test";
import { crypto } from "rts";
import { randomUUID } from "node:crypto";

const u1: string = crypto.random_uuid();
const u2: string = crypto.random_uuid();
const u3: string = randomUUID();
const u4: string = randomUUID();

const len1: i64 = u1.length;
const len3: i64 = u3.length;

// Diferentes em chamadas consecutivas (CSPRNG)
const distinct = u1 !== u2;

// Formato 8-4-4-4-12 com hyphens nas posicoes corretas
const hyphen8 = u1.charAt(8) === "-";
const hyphen13 = u1.charAt(13) === "-";
const hyphen18 = u1.charAt(18) === "-";
const hyphen23 = u1.charAt(23) === "-";

// Version v4: 13 deve ser '4'
const version_v4 = u1.charAt(14) === "4";

// Variant: posicao 19 deve ser 8/9/a/b
const variant_char = u1.charAt(19);
const variant_ok =
    variant_char === "8" ||
    variant_char === "9" ||
    variant_char === "a" ||
    variant_char === "b";

describe("crypto_random_uuid", () => {
    test("rts.crypto.random_uuid length 36", () => expect(len1).toBe(36));
    test("node:crypto.randomUUID length 36", () => expect(len3).toBe(36));
    test("CSPRNG: consecutive calls differ", () =>
        expect(distinct).toBe(true));
    test("hyphen at index 8", () => expect(hyphen8).toBe(true));
    test("hyphen at index 13", () => expect(hyphen13).toBe(true));
    test("hyphen at index 18", () => expect(hyphen18).toBe(true));
    test("hyphen at index 23", () => expect(hyphen23).toBe(true));
    test("version 4 marker at index 14", () =>
        expect(version_v4).toBe(true));
    test("variant 10xx marker at index 19", () =>
        expect(variant_ok).toBe(true));
});
