import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Subclasse: initializers rodam DEPOIS de super(), antes do user code

class Base {
    a: number = 10;
    constructor() {
        // Base initializer: a=10
    }
}

class Sub extends Base {
    b: number = 20;
    c: number;

    constructor() {
        super();
        // após super: a=10. depois rolam initializers de Sub: b=20.
        // Aí o user code aqui:
        this.c = this.a + this.b; // 30
    }
}

const s = new Sub();
print(`${s.a}`); // 10

print(`${s.b}`); // 20

print(`${s.c}`); // 30

describe("fixture:property_init_extends", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n20\n30\n");
  });
});
