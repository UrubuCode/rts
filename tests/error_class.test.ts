import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#214) Error / TypeError / RangeError / ReferenceError / SyntaxError
// como builtin classes JS. Mapped pra Map handle com keys
// \`message\` e \`name\`.

// 1. new Error
const e = new Error("oops");
print(e.message);          // oops
print(e.name);             // Error

// 2. TypeError
const t = new TypeError("bad arg");
print(t.message);          // bad arg
print(t.name);             // TypeError

// 3. Em throw + catch (e tem tipo Error inferido pelo throw class)
try {
  throw new RangeError("range");
} catch (e) {
  print((e as RangeError).message);  // range
  print((e as RangeError).name);     // RangeError
}

// 4. Mensagem vazia (no args)
const e2 = new Error();
print(`empty=${e2.message}`);  // empty=

// 5. Em fn user
function fail(reason: string): void {
  throw new Error(reason);
}

try {
  fail("from fn");
} catch (e) {
  print(`caught: ${(e as Error).message}`);  // caught: from fn
}

describe("error_class", () => {
  test("Error builtin com message/name", () =>
    expect(__rtsCapturedOutput).toBe(
      "oops\nError\n" +              // 1
      "bad arg\nTypeError\n" +       // 2
      "range\nRangeError\n" +        // 3
      "empty=\n" +                   // 4
      "caught: from fn\n"            // 5
    ));
});
