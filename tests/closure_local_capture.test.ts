import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Captura de local em arrow callback dentro de função user.
// Padrão natural via VarDecl — substitui gambiarra `__lifted_arrow_0()`
// que existia antes do lift de VarDecl arrow funcionar (#97 fase 3).

function setup(): void {
    let count: number = 0;
    const cb = () => {
        count = count + 1;
    };
    cb();
    cb();
    cb();
    const h = gc.string_from_i64(count);
    print(h); gc.string_free(h); // 3
}

setup();

describe("fixture:closure_local_capture", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("3\n");
  });
});
