import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

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

const h1 = gc.string_from_i64(Net.Status.Ok);
print(h1); gc.string_free(h1); // 0
const h2 = gc.string_from_i64(Net.Status.NotFound);
print(h2); gc.string_free(h2); // 404
const h3 = gc.string_from_i64(Net.Status.ServerError);
print(h3); gc.string_free(h3); // 500

describe("fixture:namespace_enum", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n404\n500\n");
  });
});
