import { describe, test, expect } from "rts:test";

const b = Buffer.from([104, 101, 108, 108, 111]);
const s = Buffer.toString(b);

const hexBuf = Buffer.from([0x96, 0x01]);
const hex = Buffer.toString(hexBuf, "hex");

const strBuf = Buffer.from("hi");
const strFromBuf = Buffer.toString(strBuf);

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
