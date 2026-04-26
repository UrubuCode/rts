import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// super.field para ler field herdado (sem getter)

class Base {
    x: number = 7;
    y: number = 13;
}

class Sub extends Base {
    sumViaSuper(): number {
        return super.x + super.y; // 20
    }
    sumViaThis(): number {
        return this.x + this.y; // 20 — equivalente
    }
}

const s = new Sub();
const h1 = gc.string_from_i64(s.sumViaSuper());
print(h1); gc.string_free(h1);
const h2 = gc.string_from_i64(s.sumViaThis());
print(h2); gc.string_free(h2);

describe("fixture:super_field_read", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("20\n20\n");
  });
});
