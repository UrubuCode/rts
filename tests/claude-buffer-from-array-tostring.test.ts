import { describe, test, expect } from "rts:test";

// `Buffer.toString(b)` (static call, `b` as an ARGUMENT) is not what decodes a
// buffer — conferido contra Node v22: `Buffer.toString` is
// `Function.prototype.toString` on the `Buffer` constructor itself, so
// `Buffer.toString(b)` answers the constructor's OWN source text and ignores
// `b` completely (`Buffer` is a legacy function, not a class — its body is
// real, printable JS). The decoding method is the INSTANCE method,
// `b.toString(encoding)`, which is what every one of these now calls.
const b = Buffer.from([104, 101, 108, 108, 111]);
const s = b.toString();

const hexBuf = Buffer.from([0x96, 0x01]);
const hex = hexBuf.toString("hex");

const strBuf = Buffer.from("hi");
const strFromBuf = strBuf.toString();

describe("Buffer.from(array) + Buffer.toString", () => {
  test("Buffer.from(numberArray) preserves bytes", () => {
    expect(b.length).toBe(5);
  });

  test("Buffer.toString(buf) decodes UTF-8", () => {
    expect(s).toBe("hello");
  });

  test("Buffer.toString(buf, 'hex') hex-encodes", () => {
    expect(hex).toBe("9601");
  });

  test("round-trips Buffer.from(string) through toString", () => {
    expect(strFromBuf).toBe("hi");
  });
});
