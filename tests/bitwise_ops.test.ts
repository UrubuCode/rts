import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

const a = 0xF0;
const b = 0x0F;

print(`and  = ${a & b}`);
print(`or   = ${a | b}`);
print(`xor  = ${a ^ b}`);
print(`not  = ${~0}`);
print(`shl  = ${1 << 4}`);
print(`shr  = ${256 >> 2}`);
print(`ushr = ${16 >>> 2}`);
print(`mask = ${(0xAB >> 4) & 0xF}`);

describe("fixture:bitwise_ops", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("and  = 0\nor   = 255\nxor  = 255\nnot  = -1\nshl  = 16\nshr  = 64\nushr = 4\nmask = 10\n");
  });
});
