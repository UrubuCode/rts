import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// catch sem binding (ES2019): `catch { ... }` em vez de `catch (e) { ... }`.

try {
    print("try");
    throw "err";
} catch {
    print("caught");
}

print("end");

describe("fixture:try_catch_no_binding", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("try\ncaught\nend\n");
  });
});
