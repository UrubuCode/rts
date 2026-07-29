import { describe, test, expect } from "rts:test";
import { createHash, randomBytes } from "node:crypto";

// `node:crypto` basics against the REAL Node surface.
//
// This file used to import `sha256` / `randomBytesBuffer` — flat helpers from
// the pre-`node:` era that mapped straight onto `rts::crypto`. They no longer
// exist: `node:crypto` now publishes the actual Node API (`createHash`,
// `createHmac`, `hash`, `randomBytes`, `randomUUID`, …), so the test was
// exercising a surface the runtime had stopped offering and bailed at import.
// Rewritten onto the real API, keeping the same known-answer vectors.

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

const h1 = createHash("sha256");
print(h1.update("hello").digest("hex"));

const h2 = createHash("sha256");
print(h2.digest("hex"));

print(`${randomBytes(16).length}`);

describe("fixture:node_crypto_basic", () => {
  test("sha256 known vectors + randomBytes length", () => {
    expect(__rtsCapturedOutput).toBe(
      "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\n" +
      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n" +
      "16\n"
    );
  });
});
