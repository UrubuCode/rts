import { describe, test, expect } from "rts:test";

let out = "";

// (#220) Date UTC getters
const d = new Date(0);  // epoch
out += d.getUTCFullYear() + "\n";   // 1970
out += d.getUTCMonth() + "\n";       // 0
out += d.getUTCDate() + "\n";        // 1
out += d.getUTCHours() + "\n";       // 0
out += d.getUTCMinutes() + "\n";     // 0
out += d.getUTCSeconds() + "\n";     // 0
out += d.getUTCMilliseconds() + "\n";// 0
out += d.getTimezoneOffset() + "\n"; // 0 (RTS sempre UTC)

// toUTCString / toDateString
out += d.toDateString() + "\n";       // 1970-01-01

describe("date_utc_getters", () => {
  test("UTC getters + extras (#220)", () => expect(out).toBe(
    "1970\n0\n1\n0\n0\n0\n0\n0\n1970-01-01\n"
  ));
});
