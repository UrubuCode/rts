import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Subclasse: initializer pode acessar field herdado do parent

class Base {
    x: number = 5;
}

class Sub extends Base {
    y: number = 0; // sobrescrito no ctor
    constructor() {
        super();
        // x ja foi initialized pelo parent (super)
        this.y = this.x * 10; // 50
    }
}

const s = new Sub();
print(`${s.x}`); // 5

print(`${s.y}`); // 50

describe("fixture:property_init_inherited_access", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("5\n50\n");
  });
});
