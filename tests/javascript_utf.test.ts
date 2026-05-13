import { describe, test, expect } from "rts:test";
import { io, string } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// "ação" em UTF-8: a (1) ç (2) ã (2) o (1) = 6 bytes / 4 chars Unicode.
// JS .length conta code units UTF-16 = 4 (chars BMP = 1 code unit cada).
// RTS agora implementa .length como UTF-16 code units (JS spec).

const exemplo = "ação";

// 1. byte_len — bytes UTF-8 do buffer
print(`bytes=${string.byte_len(exemplo)}`);

// 2. char_count — code points Unicode
print(`chars=${string.char_count(exemplo)}`);

// 3. .length em RTS — UTF-16 code units (JS spec); para "ação" = 4
print(`len=${exemplo.length}`);

// 4. ASCII puro: byte_len == char_count == length
const ascii = "abcdef";
print(`ascii_bytes=${string.byte_len(ascii)} ascii_chars=${string.char_count(ascii)} ascii_len=${ascii.length}`);

// 5. String vazia
print(`empty_bytes=${string.byte_len("")} empty_chars=${string.char_count("")}`);

// 6. char_at retorna o code point como string ASCII-safe
print(`char[0]=${string.char_at(exemplo, 0)}`);  // "a"
print(`char[3]=${string.char_at(exemplo, 3)}`);  // "o"

// 7. Concat preserva bytes UTF-8 corretamente
const dobro = exemplo + exemplo;
print(`dobro_bytes=${string.byte_len(dobro)}`);  // 12
print(`dobro_chars=${string.char_count(dobro)}`); // 8

describe("JS UTF — byte_len vs char_count vs length", () => {
  test("ASCII tem 1:1, multi-byte diverge", () => {
    expect(__rtsCapturedOutput).toBe(
      "bytes=6\n" +
      "chars=4\n" +
      "len=4\n" +
      "ascii_bytes=6 ascii_chars=6 ascii_len=6\n" +
      "empty_bytes=0 empty_chars=0\n" +
      "char[0]=a\n" +
      "char[3]=o\n" +
      "dobro_bytes=12\n" +
      "dobro_chars=8\n"
    );
  });
});
