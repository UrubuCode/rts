import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Namespace contendo enum.

namespace Net {
    export enum Status {
        Ok,
        NotFound = 404,
        ServerError = 500,
    }
}

print(`${Net.Status.Ok}`); // 0
print(`${Net.Status.NotFound}`); // 404
print(`${Net.Status.ServerError}`); // 500

describe("fixture:namespace_enum", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n404\n500\n");
  });
});
