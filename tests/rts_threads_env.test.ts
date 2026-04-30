import { describe, test, expect } from "rts:test";
import { gc, parallel } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #283 item 3: parallel.num_threads() reflete RTS_THREADS quando setado,
// senao available_parallelism. Aqui apenas validamos que retorna >= 1.

const n = parallel.num_threads();
const valid = n >= 1;
const h = gc.string_from_static(valid ? "ok" : "bad");
print(h); gc.string_free(h);

describe("fixture:rts_threads_env", () => {
  test("parallel.num_threads >= 1", () => {
    expect(__rtsCapturedOutput).toBe("ok\n");
  });
});
