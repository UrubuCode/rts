import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// `expr as ClassName` permite chamar métodos quando o tipo dinâmico
// não é conhecido estaticamente.

class Counter {
    n: number = 0;
    bump(): number {
        this.n = this.n + 1;
        return this.n;
    }
}

function makeAny(): number {
    // Retorna um number mas que na verdade é handle de Counter.
    const c = new Counter();
    return c as number; // unsafe cast — handle vira number
}

const handle = makeAny();
const c = handle as Counter; // recupera tipo
print(`${c.bump()}`); // 1

print(`${c.bump()}`); // 2

describe("fixture:type_assertion_class", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("1\n2\n");
  });
});
