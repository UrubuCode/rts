import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Ctor pode sobrescrever initializer (initializer roda primeiro)

class C {
    n: number = 1;

    constructor(arg: number) {
        // initializer rodou: this.n = 1 antes desta linha
        this.n = arg;  // sobrescreve
    }
}

const c = new C(99);
print(`${c.n}`); // 99

describe("fixture:property_init_override", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("99\n");
  });
});
