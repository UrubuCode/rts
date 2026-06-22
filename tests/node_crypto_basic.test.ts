import { describe, test, expect } from "rts:test";
import { sha256, randomBytesBuffer } from "node:crypto";
import { buffer } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #289 fase 1: node:crypto.sha256/randomBytesBuffer mapeando rts::crypto.

const h1 = sha256("hello");
print(h1);

const h2 = sha256("");
print(h2);

const buf = randomBytesBuffer(16);
const len = buffer.len(buf);
print(`${len}`);
buffer.free(buf);

describe("fixture:node_crypto_basic", () => {
  test("sha256 known vectors + randomBytesBuffer length", () => {
    expect(__rtsCapturedOutput).toBe(
      "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\n" +
      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n" +
      "16\n"
    );
  });
});
