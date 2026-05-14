import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const str = "hello";
print("0=" + str.charAt(0));
print("4=" + str.charAt(4));
print("10=" + str.charAt(10));

// Emoji fora do BMP — JS spec: charAt indexa code units UTF-16,
// nao code points. charAt(0) de "😀" retorna o high surrogate (U+D83D)
// isolado, que vira REPLACEMENT CHARACTER (U+FFFD = "�") na saida.
const emoji = "😀";
print("emoji_len=" + emoji.charCodeAt(0));
print("emoji_isolated=" + (emoji.charAt(0) === "�"));

describe("string.charAt — UTF-16 code units (#762)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "0=h\n" +
      "4=o\n" +
      "10=\n" +
      "emoji_len=55357\n" +
      "emoji_isolated=true\n"
    )
  );
});
