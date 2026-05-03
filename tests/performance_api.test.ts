import { describe, test, expect } from "rts:test";

let out = "";

// (#371) performance.now() — monotonic ms
const a = performance.now();
const b = performance.now();
out += (b >= a ? "monotonic" : "broken") + "\n";

// performance.timeOrigin — recente epoch
out += (performance.timeOrigin > 1700000000000 ? "origin-ok" : "origin-bad") + "\n";

describe("performance_api", () => {
  test("now + timeOrigin (#371)", () => expect(out).toBe(
    "monotonic\norigin-ok\n"
  ));
});
