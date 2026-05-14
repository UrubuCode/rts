import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// BigInt literals — RTS aceita mas trata como i64 (perde precisao acima de 2^63).
const small = 100n;
print("small=" + small);

const billion = 1_000_000_000n;
print("billion=" + billion);

// Com underscores
const big = 1_234_567_890n;
print("big=" + big);

describe("BigInt literals as i64 (#751)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "small=100\n" +
      "billion=1000000000\n" +
      "big=1234567890\n"
    )
  );
});
