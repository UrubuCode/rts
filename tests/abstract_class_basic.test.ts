import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Abstract class com método abstract; subclasse concreta implementa.

abstract class Shape {
    abstract area(): number;

    describe(): number {
        // Método concreto que chama o abstract — dispatch virtual
        // resolve em runtime para a implementação da subclasse.
        return this.area();
    }
}

class Square extends Shape {
    side: number = 5;
    area(): number {
        return this.side * this.side;
    }
}

const sq = new Square();
const h1 = gc.string_from_i64(sq.area());
print(h1); gc.string_free(h1); // 25

const h2 = gc.string_from_i64(sq.describe());
print(h2); gc.string_free(h2); // 25 (via dispatch virtual)

describe("fixture:abstract_class_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("25\n25\n");
  });
});
