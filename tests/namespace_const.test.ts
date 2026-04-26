import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Namespace com constants exportadas.

namespace Conf {
    export const PORT = 3000;
    export const RETRIES = 5;
}

const h1 = gc.string_from_i64(Conf.PORT);
print(h1); gc.string_free(h1); // 3000

const h2 = gc.string_from_i64(Conf.RETRIES);
print(h2); gc.string_free(h2); // 5

describe("fixture:namespace_const", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("3000\n5\n");
  });
});
