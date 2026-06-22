import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Spread literal em super(...) e super.method(...).

class Base {
    a: number;
    b: number;
    constructor(a: number, b: number) {
        this.a = a;
        this.b = b;
    }
    addBoth(x: number, y: number): number {
        return this.a + this.b + x + y;
    }
}

class Sub extends Base {
    constructor() {
        super(...[3, 7]);
    }
    callBase(): number {
        return super.addBoth(...[100, 200]);
    }
}

const s = new Sub();
print(`${s.a + s.b}`); // 10
print(`${s.callBase()}`); // 3+7+100+200 = 310

describe("fixture:spread_super", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n310\n");
  });
});
