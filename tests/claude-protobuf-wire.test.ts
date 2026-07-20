import { describe, test, expect } from "rts:test";
import { newWriter, newReader, encodeVarint, decodeVarint, WIRE_VARINT, WIRE_LEN } from "rts:protobuf";

// Buffer.from(numberArray).toString()/.toString("hex") has a pre-existing,
// unrelated gap (doesn't decode/hex-encode the bytes correctly) — hand-roll
// hex from the raw number arrays instead.
function toHex(bytes: number[]): string {
  let out = "";
  for (let i = 0; i < bytes.length; i++) {
    out += bytes[i].toString(16).padStart(2, "0");
  }
  return out;
}

function bytesOf(s: string): number[] {
  const out: number[] = [];
  for (let i = 0; i < s.length; i++) {
    out.push(s.charCodeAt(i));
  }
  return out;
}

// Encode a message like `message M { int32 id = 1; string name = 2; }`
// with id=150, name="hi" (150 varint-encodes to 0x96 0x01 per protobuf.dev's
// own worked example).
const w = newWriter();
w.writeTag(1, WIRE_VARINT);
w.writeVarint(150);
w.writeTag(2, WIRE_LEN);
w.writeBytes(bytesOf("hi"));
const encoded = w.finish();
const encodedHex = toHex(encoded);

// Decode it back field by field.
const r = newReader(encoded);
const field1 = r.readTag();
const id = r.readVarint();
const field2 = r.readTag();
const nameBytes = r.readBytes();
const nameHex = toHex(nameBytes);
const atEnd = r.readTag();

// Standalone varint helpers.
const rawVarint150Hex = toHex(encodeVarint(150));
const decoded = decodeVarint(encodeVarint(300), 0);

// Skip an unknown field.
const w2 = newWriter();
w2.writeTag(5, WIRE_VARINT);
w2.writeVarint(42);
w2.writeTag(1, WIRE_VARINT);
w2.writeVarint(7);
const msg2 = w2.finish();
const r2 = newReader(msg2);
r2.readTag(); // field 5, unknown to this reader
const skipped = r2.skip();
r2.readTag(); // field 1
const known = r2.readVarint();

describe("rts:protobuf wire format", () => {
  test("encodes matching protobuf.dev's worked example (150 -> 0x96 0x01)", () => {
    // tag(1,varint)=0x08, varint(150)=0x96 0x01, tag(2,len)=0x12, len=2, "hi"=0x68 0x69
    expect(encodedHex).toBe("08960112026869");
  });

  test("decodes id field", () => {
    expect(field1).toBe(1);
    expect(id).toBe(150);
  });

  test("decodes string field", () => {
    expect(field2).toBe(2);
    expect(nameHex).toBe("6869");
  });

  test("readTag returns -1 at end of buffer", () => {
    expect(atEnd).toBe(-1);
  });

  test("standalone encodeVarint matches writer output", () => {
    expect(rawVarint150Hex).toBe("9601");
  });

  test("standalone decodeVarint round-trips", () => {
    expect(decoded.value).toBe(300);
  });

  test("skip() advances past an unknown field", () => {
    expect(skipped).toBe(true);
    expect(known).toBe(7);
  });
});
