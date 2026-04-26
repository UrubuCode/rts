import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Método async em classe.

class Service {
    base: number = 100;

    async getData(): Promise<number> {
        return this.base;
    }

    async total(): Promise<number> {
        const d = await this.getData();
        return d + 7;
    }
}

const s = new Service();
const r = s.total();
const h = gc.string_from_i64(r as number);
print(h); gc.string_free(h); // 107

describe("fixture:async_class", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("107\n");
  });
});
