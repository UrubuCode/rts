import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const str = "hello";
print("0=" + str.charAt(0));
print("4=" + str.charAt(4));
print("10=" + str.charAt(10));

// Emoji fora do BMP — JS spec: charAt indexa code units UTF-16, nao code
// points. charAt(0) de "😀" retorna o high surrogate (U+D83D) ISOLADO como
// valor de string — nao U+FFFD. A afirmacao anterior aqui (=== "�") era
// falsa: conferido contra Node v22 (`node -e`), `"😀".charAt(0) === "�"` e'
// `false`, e o valor certo e' `"😀".charAt(0).charCodeAt(0) === 0xD83D` —
// que e' o que esta asserção agora checa. `"😀".charAt(0) === "\uD83D"`
// TAMBEM e' `true` no Node, mas essa comparação exige que um LITERAL de
// string com surrogate isolado sobreviva o parser deste motor sem virar
// U+FFFD — capacidade que este motor nao tem (o texto de um literal passa
// por um `&str` Rust, que e' UTF-8 e nao representa um surrogate solto), e
// nao e' o que este teste existe para cobrir. `charCodeAt` evita a mesma
// pergunta, porque le o code unit sem reconstruir um literal problematico.
const emoji = "😀";
print("emoji_len=" + emoji.charCodeAt(0));
print("emoji_isolated=" + (emoji.charAt(0).charCodeAt(0) === 0xD83D));

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
