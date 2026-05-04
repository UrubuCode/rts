import { describe, test, expect } from "rts:test";

let out = "";

// (#220) Date.UTC(year, month, day?, ...) — ms since epoch
const epoch = Date.UTC(1970, 0, 1, 0, 0, 0, 0);
out += epoch + "\n";   // 0

const y2k = Date.UTC(2000, 0, 1);
out += (y2k > 946684800000 - 1 && y2k < 946684800000 + 1 ? "y2k-ok" : "y2k-bad:" + y2k) + "\n";

const recent = Date.UTC(2024, 5, 15);
out += (recent > 1700000000000 ? "recent-ok" : "recent-bad") + "\n";

// Roundtrip via new Date(ms).
const d = new Date(epoch);
out += d.getUTCFullYear() + "\n";   // 1970
out += d.getUTCMonth() + "\n";       // 0

describe("date_utc_static", () => {
  test("Date.UTC(year, month, day?, ...) (#220)", () => expect(out).toBe(
    "0\ny2k-ok\nrecent-ok\n1970\n0\n"
  ));
});
