import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. Operacoes basicas (caso historico)
const a = 0xF0;
const b = 0x0F;
print(`and  = ${a & b}`);   // 0
print(`or   = ${a | b}`);   // 255
print(`xor  = ${a ^ b}`);   // 255
print(`not  = ${~0}`);      // -1
print(`shl  = ${1 << 4}`);  // 16
print(`shr  = ${256 >> 2}`);// 64
print(`ushr = ${16 >>> 2}`);// 4
print(`mask = ${(0xAB >> 4) & 0xF}`); // 10

// 2. Operacoes em variaveis (peephole imm vs runtime)
const x: i32 = 0xCAFE;
print(`v_and = ${x & 0xFF}`);     // 0xFE = 254
print(`v_or  = ${x | 0xF000}`);   // 0xFAFE = 64254
print(`v_xor = ${x ^ 0xFFFF}`);   // 0x3501 = 13569

// 3. NOT — complemento de dois
print(`~5  = ${~5}`);     // -6
print(`~-1 = ${~-1}`);    // 0
print(`~~7 = ${~~7}`);    // 7  (truncate idiom)

// 4. Shift sinal preservado
print(`shr_neg = ${-8 >> 1}`);    // -4

// 5. Idioms comuns — set/clear/toggle/test bit
const FLAGS = 0;
const SET3   = FLAGS | (1 << 3);
const SET3_5 = SET3  | (1 << 5);
const CLR3   = SET3_5 & ~(1 << 3);
const TGL5   = CLR3   ^ (1 << 5);
print(`set    = ${SET3_5}`);          // 0b101000 = 40
print(`clear3 = ${CLR3}`);            // 0b100000 = 32
print(`toggle = ${TGL5}`);            // 0
print(`test3  = ${(SET3_5 & (1 << 3)) !== 0}`);  // true
print(`test4  = ${(SET3_5 & (1 << 4)) !== 0}`);  // false

// 6. AND com mascara em literal hex (regressao common)
print(`hi_byte = ${(0xABCD >> 8) & 0xFF}`);   // 0xAB = 171
print(`lo_byte = ${0xABCD & 0xFF}`);          // 0xCD = 205

// 7. Associatividade (left-to-right)
print(`assoc1 = ${1 | 2 | 4}`);       // 7
print(`assoc2 = ${0xFF & 0x0F & 0x03}`);// 3

describe("bitwise — vars, NOT, sinal em shr, idioms", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "and  = 0\nor   = 255\nxor  = 255\nnot  = -1\nshl  = 16\nshr  = 64\nushr = 4\nmask = 10\n" +
      "v_and = 254\nv_or  = 64254\nv_xor = 13569\n" +
      "~5  = -6\n~-1 = 0\n~~7 = 7\n" +
      "shr_neg = -4\n" +
      "set    = 40\nclear3 = 32\ntoggle = 0\ntest3  = true\ntest4  = false\n" +
      "hi_byte = 171\nlo_byte = 205\n" +
      "assoc1 = 7\nassoc2 = 3\n"
    );
  });
});
