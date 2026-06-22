import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Cada instância recebe sua própria cópia dos initializers

class C {
    n: number = 100;
}

const a = new C();
const b = new C();
const k = new C();

a.n = 1;
b.n = 2;
// k.n permanece 100 (initializer)

print(`${a.n}`); // 1
print(`${b.n}`); // 2
print(`${k.n}`); // 100

describe("fixture:property_init_multi_instance", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("1\n2\n100\n");
  });
});
