import { describe, test, expect } from "rts:test";
import { io, string } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

const a = "hello";
const b = "hello";
const c = "world";
print(`a == b: ${a == b}`);
print(`a == c: ${a == c}`);
print(`a != c: ${a != c}`);

// char_at retorna string handle
const s = "abc";
const ch = string.char_at(s, 1);
print(`ch == "b": ${ch == "b"}`);
print(`ch == "x": ${ch == "x"}`);

// Strings vazias
const e1 = "";
const e2 = "";
print(`empty == empty: ${e1 == e2}`);

describe("fixture:string_eq", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("a == b: 1\na == c: 0\na != c: 1\nch == \"b\": 1\nch == \"x\": 0\nempty == empty: 1\n");
  });
});
