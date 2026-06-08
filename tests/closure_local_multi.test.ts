import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Múltiplas locais capturadas no mesmo callback.

function setup(): void {
    let a: number = 100;
    let b: number = 200;
    const cb = () => {
        a = a + 1;
        b = b + 10;
    };
    cb();
    cb();
    const ha = gc.string_from_i64(a);
    print(ha); gc.string_free(ha); // 102
    const hb = gc.string_from_i64(b);
    print(hb); gc.string_free(hb); // 220
}

setup();

describe("fixture:closure_local_multi", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("102\n220\n");
  });
});
