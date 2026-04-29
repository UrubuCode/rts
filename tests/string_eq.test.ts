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
    expect(__rtsCapturedOutput).toBe("a == b: true\na == c: false\na != c: true\nch == \"b\": true\nch == \"x\": false\nempty == empty: true\n");
  });
});
