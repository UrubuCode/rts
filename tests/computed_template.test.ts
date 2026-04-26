import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Template literal sem interpolação: [`name`]() ≡ name()

class C {
    [`hello`](): string {
        return "world";
    }
}

const c = new C();
print(c.hello());

describe("fixture:computed_template", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("world\n");
  });
});
