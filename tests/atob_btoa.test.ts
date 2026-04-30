import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #372: atob / btoa globais (base64).
// Implementacao em src/namespaces/globals/text_encoding/abi.rs sobre crypto.base64_*.

print(btoa("hello"));        // aGVsbG8=
print(btoa(""));             // (vazio)
print(btoa("foobar"));       // Zm9vYmFy
print(atob("aGVsbG8="));     // hello
print(atob(""));             // (vazio)
print(atob("Zm9vYmFy"));     // foobar
print(atob(btoa("round-trip ascii"))); // round-trip ascii

describe("fixture:atob_btoa", () => {
  test("btoa/atob globais — encode/decode/round-trip", () => {
    expect(__rtsCapturedOutput).toBe(
      "aGVsbG8=\n\nZm9vYmFy\nhello\n\nfoobar\nround-trip ascii\n"
    );
  });
});
