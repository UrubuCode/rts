import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// "ação" em UTF-8: a (1) ç (2) ã (2) o (1) = 6 bytes / 4 chars Unicode.
// JS .length conta code units UTF-16 = 4 (chars BMP = 1 code unit cada).
// RTS agora implementa .length como UTF-16 code units (JS spec).
//
// As tres medidas sao expressas em JS puro (o namespace rts:string foi drenado;
// byte_len/char_count nao tinham par na value-class String, mas tem em JS):
//   bytes UTF-8   -> new TextEncoder().encode(s).length
//   code points   -> Array.from(s).length   (itera por code point, nao por unit)
//   code units    -> s.length
// Mesma premissa e mesmos valores esperados de antes — so a expressao mudou.

const exemplo = "ação";

// 1. bytes UTF-8 do buffer
print(`bytes=${new TextEncoder().encode(exemplo).length}`);

// 2. code points Unicode
print(`chars=${Array.from(exemplo).length}`);

// 3. .length em RTS — UTF-16 code units (JS spec); para "ação" = 4
print(`len=${exemplo.length}`);

// 4. ASCII puro: bytes == code points == code units
const ascii = "abcdef";
print(`ascii_bytes=${new TextEncoder().encode(ascii).length} ascii_chars=${Array.from(ascii).length} ascii_len=${ascii.length}`);

// 5. String vazia
print(`empty_bytes=${new TextEncoder().encode("").length} empty_chars=${Array.from("").length}`);

// 6. charAt indexa por code unit UTF-16; em "ação" (tudo BMP) coincide com o
//    indice por code point
print(`char[0]=${exemplo.charAt(0)}`);  // "a"
print(`char[3]=${exemplo.charAt(3)}`);  // "o"

// 7. Concat preserva bytes UTF-8 corretamente
const dobro = exemplo + exemplo;
print(`dobro_bytes=${new TextEncoder().encode(dobro).length}`);  // 12
print(`dobro_chars=${Array.from(dobro).length}`); // 8

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
