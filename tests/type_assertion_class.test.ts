import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

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
const h1 = gc.string_from_i64(c.bump());
print(h1); gc.string_free(h1); // 1

const h2 = gc.string_from_i64(c.bump());
print(h2); gc.string_free(h2); // 2

describe("fixture:type_assertion_class", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("1\n2\n");
  });
});
