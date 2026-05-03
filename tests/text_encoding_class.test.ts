import { describe, test, expect } from "rts:test";

let out = "";

// (#374) TextEncoder/TextDecoder roundtrip
const enc = new TextEncoder();
const dec = new TextDecoder();

const round = dec.decode(enc.encode("hello"));
out += round + "\n";    // hello

const round2 = dec.decode(enc.encode("rts works"));
out += round2 + "\n";   // rts works

describe("text_encoding_class", () => {
  test("encode/decode roundtrip (#374)", () => expect(out).toBe(
    "hello\nrts works\n"
  ));
});
