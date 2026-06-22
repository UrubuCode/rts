import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Computed property name (field): `["x"]: number = 42`

class C {
    ["count"]: number = 0;

    ["bump"](): void {
        this.count = this.count + 1;
    }
}

const c = new C();
c.bump();
c.bump();
c.bump();

print(`${c.count}`); // 3

describe("fixture:computed_property_str", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("3\n");
  });
});
