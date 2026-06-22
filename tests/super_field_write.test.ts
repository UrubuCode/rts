import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// super.field = v escreve no field herdado

class Base {
    x: number = 0;
}

class Sub extends Base {
    setBase(v: number): void {
        super.x = v;
    }
    setBaseCompound(): void {
        super.x = super.x + 100;
    }
}

const s = new Sub();
s.setBase(42);
print(`${s.x}`); // 42

s.setBaseCompound(); // 42 + 100
print(`${s.x}`); // 142

describe("fixture:super_field_write", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\n142\n");
  });
});
