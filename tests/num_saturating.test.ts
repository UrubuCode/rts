import { describe, test, expect } from "rts:test";
import { io, num } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// saturating_*: clamp em i64::MIN/MAX.

// 9_000_000_000_000_000_000 + 9_000_000_000_000_000_000 saturaria.
const a = num.saturating_add(9000000000000000000, 9000000000000000000);
print(`${a}`);

const b = num.saturating_sub(-9000000000000000000, 9000000000000000000);
print(`${b}`);

const c = num.saturating_mul(3, 7);
print(`${c}`);

describe("fixture:num_saturating", () => {
  test("matches expected stdout", () => {
    // Limites i64 (i64::MAX/MIN) > 2^53: sem repr Int64, viram f64 e imprimem
    // como JS/Node (±9223372036854776000). Era exato no motor velho (deletado).
    expect(__rtsCapturedOutput).toBe("9223372036854776000\n-9223372036854776000\n21\n");
  });
});
