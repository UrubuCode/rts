import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Private field acessível só dentro da classe

class Counter {
    #count: number = 0;

    inc(): void {
        this.#count = this.#count + 1;
    }

    value(): number {
        return this.#count;
    }
}

const c = new Counter();
const h0 = gc.string_from_i64(c.value()); print(h0); gc.string_free(h0); // 0

c.inc();
c.inc();
c.inc();

const h3 = gc.string_from_i64(c.value()); print(h3); gc.string_free(h3); // 3

describe("fixture:private_field_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n3\n");
  });
});
