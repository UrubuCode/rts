import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

function main() {
  // try com throw direto
  try {
    print("a");
    throw "first";
  } catch (e) {
    print(`caught: ${e}`);
  } finally {
    print("finally1");
  }

  // try sem throw
  try {
    print("b");
  } catch (e) {
    print(`nope: ${e}`);
  }

  // try sem catch (so finally)
  try {
    print("c");
  } finally {
    print("finally3");
  }

  print("end");
}

main();

describe("fixture:try_catch", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("a\ncaught: first\nfinally1\nb\nc\nfinally3\nend\n");
  });
});
