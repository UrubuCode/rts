import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// readonly: pode ser atribuído no constructor

class Point {
    readonly x: number;
    readonly y: number;
    constructor(x: number, y: number) {
        this.x = x; // OK no ctor
        this.y = y;
    }
    sum(): number { return this.x + this.y; }
}

const p = new Point(7, 13);
const h = gc.string_from_i64(p.sum());
print(h); gc.string_free(h); // 20

describe("fixture:readonly_field_ctor", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("20\n");
  });
});
