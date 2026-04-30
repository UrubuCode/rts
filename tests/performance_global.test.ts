import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #371: performance.now() + performance.timeOrigin
// Implementacao em src/namespaces/globals/performance/instance.rs.

const t0 = performance.now();
// gasta um pouco de tempo
let acc: f64 = 0.0;
for (let i: i64 = 0; i < 100000; i = i + 1) {
  acc = acc + 1.0;
}
const t1 = performance.now();

print(t0 >= 0.0 ? "now0_ok" : "now0_fail");
print(t1 >= t0 ? "monotonic_ok" : "monotonic_fail");
print(acc === 100000.0 ? "loop_ok" : "loop_fail");

const origin = performance.timeOrigin;
// Unix ms desde 2020 (~1577836800000) — sanity check
print(origin > 1577836800000.0 ? "origin_ok" : "origin_fail");

describe("fixture:performance_global", () => {
  test("performance.now() monotonico + timeOrigin sane", () => {
    expect(__rtsCapturedOutput).toBe(
      "now0_ok\nmonotonic_ok\nloop_ok\norigin_ok\n"
    );
  });
});
