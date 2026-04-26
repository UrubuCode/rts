import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

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
const h1 = gc.string_from_i64(b.area());
print(h1); gc.string_free(h1); // 16

const h2 = gc.string_from_i64(b.describe());
print(h2); gc.string_free(h2); // 116

describe("fixture:abstract_chain", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("16\n116\n");
  });
});
