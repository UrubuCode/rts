import { describe, test, expect } from "rts:test";
import { io } from "rts";

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
print(`${c.value()}`); // 0

c.inc();
c.inc();
c.inc();

print(`${c.value()}`); // 3

describe("fixture:private_field_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n3\n");
  });
});
