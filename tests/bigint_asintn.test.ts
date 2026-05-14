import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// BigInt.asIntN — wrap signed em N bits.
print("i8_255=" + BigInt.asIntN(8, 255n));     // -1
print("i8_128=" + BigInt.asIntN(8, 128n));     // -128
print("i8_127=" + BigInt.asIntN(8, 127n));     // 127
print("i16_neg=" + BigInt.asIntN(16, 65535n)); // -1

// BigInt.asUintN — wrap unsigned em N bits.
print("u8_neg1=" + BigInt.asUintN(8, -1n));    // 255
print("u8_256=" + BigInt.asUintN(8, 256n));    // 0
print("u16_65536=" + BigInt.asUintN(16, 65536n)); // 0

describe("BigInt.asIntN / asUintN (#742)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "i8_255=-1\n" +
      "i8_128=-128\n" +
      "i8_127=127\n" +
      "i16_neg=-1\n" +
      "u8_neg1=255\n" +
      "u8_256=0\n" +
      "u16_65536=0\n"
    )
  );
});
