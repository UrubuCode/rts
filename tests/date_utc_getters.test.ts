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
// (cross-runtime #172) getTimezoneOffset agora retorna offset real do
// sistema (em minutos, JS spec). Validamos so' que e' um numero, nao
// um valor fixo (varia por TZ do host).
const tz = d.getTimezoneOffset();
out += (typeof tz === "number") + "\n"; // true

// toUTCString / toDateString
// (PR #1203) toDateString agora segue JS spec: "Thu Jan 01 1970"
out += d.toDateString() + "\n";

describe("date_utc_getters", () => {
  test("UTC getters + extras (#220)", () => expect(out).toBe(
    "1970\n0\n1\n0\n0\n0\n0\ntrue\nThu Jan 01 1970\n"
  ));
});
