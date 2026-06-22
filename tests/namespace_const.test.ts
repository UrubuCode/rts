import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Namespace com constants exportadas.

namespace Conf {
    export const PORT = 3000;
    export const RETRIES = 5;
}

print(`${Conf.PORT}`); // 3000

print(`${Conf.RETRIES}`); // 5

describe("fixture:namespace_const", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("3000\n5\n");
  });
});
