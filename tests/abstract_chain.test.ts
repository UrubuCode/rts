import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Cadeia abstract → abstract → concreto

abstract class Shape {
    abstract area(): number;
}

abstract class ColoredShape extends Shape {
    abstract describe(): number;
    // não implementa area — herda como abstract
}

class Box extends ColoredShape {
    side: number = 4;
    area(): number { return this.side * this.side; }
    describe(): number { return 100 + this.area(); }
}

const b = new Box();
print(`${b.area()}`); // 16

print(`${b.describe()}`); // 116

describe("fixture:abstract_chain", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("16\n116\n");
  });
});
