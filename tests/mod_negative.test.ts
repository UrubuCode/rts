import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// JS preserva sinal do dividendo. RTS tinha peephole `x % 2^k -> x & MASK`
// que ignorava o sinal — agora corrigido com adj branchless (#297).
print(`${-7 % 4}`);
print(`${-1 % 4}`);
print(`${-8 % 4}`);
print(`${7 % 4}`);
print(`${0 % 4}`);
print(`${-7 % 8}`);
print(`${-15 % 16}`);
print(`${-16 % 16}`);

describe("mod_negative", () => {
  test("preserves sign of dividend", () =>
    expect(__rtsCapturedOutput).toBe("-3\n-1\n0\n3\n0\n-7\n-15\n0\n"));
});
