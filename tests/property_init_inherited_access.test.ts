import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

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
const hx = gc.string_from_i64(s.x);
print(hx); gc.string_free(hx); // 5

const hy = gc.string_from_i64(s.y);
print(hy); gc.string_free(hy); // 50

describe("fixture:property_init_inherited_access", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("5\n50\n");
  });
});
