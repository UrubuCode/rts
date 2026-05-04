import { describe, test, expect } from "rts:test";

let out = "";

// (#208) encodeURIComponent / decodeURIComponent globais
out += encodeURIComponent("hello world") + "\n";       // hello%20world
out += encodeURIComponent("a&b=c") + "\n";             // a%26b%3Dc
out += decodeURIComponent("hello%20world") + "\n";     // hello world
out += decodeURIComponent("a%26b%3Dc") + "\n";         // a&b=c

// roundtrip
const original = "x=1&y=2 z";
const enc = encodeURIComponent(original);
const dec = decodeURIComponent(enc);
out += dec + "\n";

describe("uri_encode", () => {
  test("encodeURIComponent/decodeURIComponent (#208)", () => expect(out).toBe(
    "hello%20world\na%26b%3Dc\nhello world\na&b=c\nx=1&y=2 z\n"
  ));
});
