import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

print(`hello world`);

const name = "RTS";
print(`hello ${name}`);

const n = 42;
print(`answer is ${n}`);

const pi = 3.14;
print(`pi = ${pi}`);

const x = 10;
const y = 20;
print(`${x} + ${y} = ${x + y}`);

print(`[${name}]`);

const greet = `hi ` + name;
print(greet);

describe("fixture:template_literals", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("hello world\nhello RTS\nanswer is 42\npi = 3.14\n10 + 20 = 30\n[RTS]\nhi RTS\n");
  });
});
