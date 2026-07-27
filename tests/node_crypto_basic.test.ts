import { describe, test, expect } from "rts:test";
import { createHash, randomBytes } from "node:crypto";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #289 — SHA-256 e bytes aleatorios pela API do Node.
//
// Migrado de `sha256(s)`/`randomBytesBuffer(n)`, atalhos so-do-RTS que foram
// drenados: no Node se escreve `createHash("sha256").update(s).digest("hex")` e
// `randomBytes(n)`. Os vetores conhecidos sao os mesmos (conferidos com
// `sha256sum`).

const h1 = createHash("sha256").update("hello").digest("hex");
print(h1);

const h2 = createHash("sha256").update("").digest("hex");
print(h2);

const buf = randomBytes(16);
print(`${buf.length}`);

describe("fixture:node_crypto_basic", () => {
  test("sha256 known vectors + randomBytes length", () => {
    expect(__rtsCapturedOutput).toBe(
      "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\n" +
      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n" +
      "16\n"
    );
  });
});
