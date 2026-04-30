import { describe, test, expect } from "rts:test";
import { gc, math } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #281: PRNG state migrado de `static mut` global para thread_local.
// math.seed() determina sequencia, math.random_f64() avanca estado por-thread.

math.seed(42);
const r1 = math.random_f64();
const r2 = math.random_f64();

math.seed(42);
const r3 = math.random_f64();
const r4 = math.random_f64();

// Mesma seed → mesma sequencia.
const same = (r1 === r3) && (r2 === r4);
const ok = gc.string_from_static(same ? "ok" : "fail");
print(ok); gc.string_free(ok);

// Sequencia avanca: r1 != r2.
const advance = gc.string_from_static(r1 !== r2 ? "advance" : "stuck");
print(advance); gc.string_free(advance);

// Range esta em [0, 1).
const inrange = (r1 >= 0.0 && r1 < 1.0) && (r2 >= 0.0 && r2 < 1.0);
const range = gc.string_from_static(inrange ? "range" : "out");
print(range); gc.string_free(range);

describe("fixture:math_random_thread_local", () => {
  test("seed determinism + range + thread-local state", () => {
    expect(__rtsCapturedOutput).toBe("ok\nadvance\nrange\n");
  });
});
