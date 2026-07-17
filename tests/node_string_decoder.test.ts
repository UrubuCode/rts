import { describe, test, expect } from "rts:test";
import { StringDecoder } from "node:string_decoder";

// € = U+20AC = UTF-8 E2 82 AC ; 😀 = U+1F600 = UTF-8 F0 9F 98 80

// ---- utf8 whole ------------------------------------------------------------
const d1: any = new StringDecoder("utf8");
const w1 = d1.write(new Uint8Array([0xE2, 0x82, 0xAC]));

// ---- utf8 boundary split (3-byte char across 3 writes) ---------------------
const d2: any = new StringDecoder("utf8");
const s2a = d2.write(new Uint8Array([0xE2]));
const s2b = d2.write(new Uint8Array([0x82]));
const s2c = d2.write(new Uint8Array([0xAC]));

// ---- utf8 4-byte split -----------------------------------------------------
const d3: any = new StringDecoder("utf8");
const s3a = d3.write(new Uint8Array([0xF0, 0x9F]));
const s3b = d3.write(new Uint8Array([0x98, 0x80]));

// ---- string input (implicit Buffer.from) -----------------------------------
const d4: any = new StringDecoder("utf8");
const w4 = d4.write("hello");

// ---- utf16le boundary ------------------------------------------------------
// "hi" utf16le = 68 00 69 00 ; split mid-unit
const d5: any = new StringDecoder("utf16le");
const s5a = d5.write(new Uint8Array([0x68, 0x00, 0x69]));
const s5b = d5.write(new Uint8Array([0x00]));

// ---- base64 ----------------------------------------------------------------
const d6: any = new StringDecoder("base64");
const s6 = d6.write(new Uint8Array([0x61, 0x62, 0x63]));

// ---- end() flushes incomplete as U+FFFD ------------------------------------
const d7: any = new StringDecoder("utf8");
d7.write(new Uint8Array([0xE2, 0x82]));
const e7 = d7.end();

// ---- end(buffer) -----------------------------------------------------------
const d8: any = new StringDecoder("utf8");
const e8 = d8.end(new Uint8Array([0x41, 0x42]));

// ---- reuse after end -------------------------------------------------------
const d9: any = new StringDecoder("utf8");
d9.write(new Uint8Array([0xE2]));
d9.end();
const r9 = d9.write(new Uint8Array([0x41]));

// ---- encoding getter -------------------------------------------------------
const encName = new StringDecoder("utf16le").encoding;

// ---- text(buffer, offset) --------------------------------------------------
const d10: any = new StringDecoder("utf8");
const t10 = d10.text(new Uint8Array([0x78, 0x79, 0x7A]), 1);

describe("node:string_decoder", () => {
  test("utf8 whole char", () => { expect(w1).toBe("€"); });
  test("utf8 boundary split", () => {
    expect(s2a).toBe("");
    expect(s2b).toBe("");
    expect(s2c).toBe("€");
  });
  test("utf8 4-byte split", () => {
    expect(s3a).toBe("");
    expect(s3b).toBe("😀");
  });
  test("string input", () => { expect(w4).toBe("hello"); });
  test("utf16le boundary", () => {
    expect(s5a).toBe("h");
    expect(s5b).toBe("i");
  });
  test("base64", () => { expect(s6).toBe("YWJj"); });
  test("end flushes incomplete", () => { expect(e7).toBe("�"); });
  test("end(buffer)", () => { expect(e8).toBe("AB"); });
  test("reuse after end", () => { expect(r9).toBe("A"); });
  test("encoding getter", () => { expect(encName).toBe("utf16le"); });
  test("text(buffer, offset)", () => { expect(t10).toBe("yz"); });
});
