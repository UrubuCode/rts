import { describe, test, expect } from "rts:test";
import { gc } from "rts";
import { formatHex, formatBin, formatOct, parseInt } from "node:util";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #288 fase 1 — node:util mapeando rts::fmt.

const hex = formatHex(255);
print(hex); gc.string_free(hex);

const bin = formatBin(10);
print(bin); gc.string_free(bin);

const oct = formatOct(8);
print(oct); gc.string_free(oct);

const n = parseInt("42");
const ns = gc.string_from_i64(n);
print(ns); gc.string_free(ns);

describe("fixture:node_util_basic", () => {
  test("formatHex / formatBin / formatOct / parseInt", () => {
    expect(__rtsCapturedOutput).toBe("0xff\n0b1010\n0o10\n42\n");
  });
});
