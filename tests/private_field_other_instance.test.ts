import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Outra instância da MESMA classe pode acessar #field

class Vec {
    #x: number = 0;
    #y: number = 0;
    constructor(x: number, y: number) {
        this.#x = x;
        this.#y = y;
    }
    addTo(other: Vec): number {
        return this.#x + other.#x + this.#y + other.#y;
    }
}

const a = new Vec(1, 2);
const b = new Vec(10, 20);
const h = gc.string_from_i64(a.addTo(b));
print(h); gc.string_free(h); // 1+10+2+20 = 33

describe("fixture:private_field_other_instance", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("33\n");
  });
});
