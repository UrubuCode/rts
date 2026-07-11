// node:string_decoder — full-surface parity test (incremental byte decoder).
import { describe, test, expect } from "rts:test";
import { StringDecoder } from "node:string_decoder";

// --- utf8 split multibyte (€ = E2 82 AC) ------------------------------------
const d1 = new StringDecoder("utf8");
const s1a = d1.write(new Uint8Array([0xe2, 0x82])); // incomplete → ""
const s1b = d1.write(new Uint8Array([0xac])); // completes → "€"

// full string in one write
const d2 = new StringDecoder("utf8");
const s2 = d2.write(new Uint8Array([0x68, 0x69])); // "hi"

// string input (Node re-encodes to UTF-8 bytes)
const d3 = new StringDecoder("utf8");
const s3 = d3.write("héllo");

// --- end() flushes incomplete as U+FFFD -------------------------------------
const d4 = new StringDecoder("utf8");
const s4a = d4.write(new Uint8Array([0xe2, 0x82])); // incomplete
const s4b = d4.end(); // → U+FFFD
const s4ok = s4a === "" && s4b === "�";

// --- encoding property + normalization --------------------------------------
const encU = new StringDecoder("utf8").encoding;
const encU16 = new StringDecoder("ucs2").encoding; // normalized → utf16le
const encHex = new StringDecoder("hex").encoding;

// --- utf16le split (surrogate pair for 𝌆 = D834 DF06 → LE bytes) ------------
const d5 = new StringDecoder("utf16le");
const s5a = d5.write(new Uint8Array([0x34, 0xd8])); // high surrogate only → ""
const s5b = d5.write(new Uint8Array([0x06, 0xdf])); // low surrogate → "𝌆"

// --- base64 (3-byte groups) -------------------------------------------------
const d6 = new StringDecoder("base64");
const s6a = d6.write(new Uint8Array([0x68, 0x65])); // 2 of 3 bytes → ""
const s6b = d6.write(new Uint8Array([0x6c])); // completes group → "aGVs"

// --- hex / latin1 -----------------------------------------------------------
const d7 = new StringDecoder("hex");
const s7 = d7.write(new Uint8Array([0xab, 0xcd]));
const d8 = new StringDecoder("latin1");
const s8 = d8.write(new Uint8Array([0xe9])); // é in latin1

// --- unknown encoding throws ------------------------------------------------
let threw = false;
try {
    const bad = new StringDecoder("bogus");
    bad.write("x");
} catch (e) {
    threw = true;
}

// --- UNTRACKED receiver (array element): the object-backed-class runtime
// dispatch path (write/end resolved via the value's __rts_class tag, not a
// statically-proven class). Split multibyte proves the instance state persists.
const arr = [new StringDecoder("utf8")];
const dArr = arr[0];
const sArrA = dArr.write(new Uint8Array([0xe2, 0x82])); // incomplete → ""
const sArrB = dArr.end(new Uint8Array([0xac])); // completes then flushes → "€"

describe("node:string_decoder full surface", () => {
    test("utf8 split part1 empty", () => expect(s1a).toBe(""));
    test("utf8 split completes", () => expect(s1b).toBe("€"));
    test("utf8 full write", () => expect(s2).toBe("hi"));
    test("utf8 string input", () => expect(s3).toBe("héllo"));
    test("end flushes U+FFFD", () => expect(s4ok).toBe(true));
    test("encoding utf8", () => expect(encU).toBe("utf8"));
    test("encoding ucs2→utf16le", () => expect(encU16).toBe("utf16le"));
    test("encoding hex", () => expect(encHex).toBe("hex"));
    test("utf16le split part1", () => expect(s5a).toBe(""));
    test("utf16le surrogate completes", () => expect(s5b).toBe("𝌆"));
    test("base64 partial", () => expect(s6a).toBe(""));
    test("base64 completes", () => expect(s6b).toBe("aGVs"));
    test("hex", () => expect(s7).toBe("abcd"));
    test("latin1", () => expect(s8).toBe("é"));
    test("unknown encoding throws", () => expect(threw).toBe(true));
    test("untracked receiver write holds partial", () => expect(sArrA).toBe(""));
    test("untracked receiver end completes multibyte", () => expect(sArrB).toBe("€"));
});
