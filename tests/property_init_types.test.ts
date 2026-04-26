import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Initializers com tipos variados: string, bool, number

class C {
    name: string = "world";
    flag: boolean = true;
    count: number = 7;
}

const c = new C();
print(c.name); // world
if (c.flag) { print("on"); } else { print("off"); }
print(c.count == 7 ? "seven" : "not seven");

describe("fixture:property_init_types", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("world\non\nseven\n");
  });
});
