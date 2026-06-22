import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Private #x e public x coexistem (mangling separado)

class C {
    x: number = 100;
    #x: number = 999;

    bothValues(): number {
        return this.x + this.#x; // 1099
    }
}

const c = new C();
print(`${c.bothValues()}`);

print(`${c.x}`); // 100 (público)

describe("fixture:private_field_no_collision", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("1099\n100\n");
  });
});
