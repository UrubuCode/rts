import { describe, test, expect } from "rts:test";
import { formatHex, formatBin, formatOct, parseInt } from "node:util";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #288 fase 1 — node:util mapeando rts::fmt.

const hex = formatHex(255);
print(hex);

const bin = formatBin(10);
print(bin);

const oct = formatOct(8);
print(oct);

const n = parseInt("42");
print(`${n}`);

describe("fixture:node_util_basic", () => {
  test("formatHex / formatBin / formatOct / parseInt", () => {
    expect(__rtsCapturedOutput).toBe("0xff\n0b1010\n0o10\n42\n");
  });
});
