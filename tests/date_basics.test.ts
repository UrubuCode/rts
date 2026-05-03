import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// new Date(0).toISOString() → "1970-01-01T00:00:00.000Z"
const epoch = new Date(0);
print(epoch.toISOString());

// Date.now() retorna timestamp grande positivo
const now = Date.now();
print((now > 1700000000000) ? "now-ok" : "now-bad");

// new Date("2024-06-15").getMonth() → 5
const d = new Date("2024-06-15");
print(d.getMonth().toString());

// new Date().getFullYear() retorna ano atual (>= 2025)
const yr = new Date().getFullYear();
print((yr >= 2025) ? "year-ok" : "year-bad");

describe("date_basics", () => {
  test("acceptance criteria #220", () => expect(out).toBe(
    "1970-01-01T00:00:00.000Z\nnow-ok\n5\nyear-ok\n"
  ));
});
