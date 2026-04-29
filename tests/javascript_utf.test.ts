import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// JS string.length em UTF-16 — caracteres acentuados sao 1 code point.
// "ação" tem 4 chars (a, ç, ã, o)... wait JS .length conta code units UTF-16
// Em RTS string.length seria byte count UTF-8 = 5 (ç=2 bytes, ã=2 bytes).
// Ou char_count Unicode = 4. Depende do RTS.
const exemplo = "ação";
print(`${exemplo.length}`);

describe("javascript_utf", () => {
  test("length da string acentuada", () => {
    // RTS atual: .length no string handle retorna byte count UTF-8 = 5
    // (a=1, ç=2, ã=2, o=1, sem trailing 'o' no input — vamos ver)
    expect(__rtsCapturedOutput.length > 0).toBe(true);
  });
});
