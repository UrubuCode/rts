import { describe, test, expect } from "rts:test";

let out = "";

const d = new Date(0);

// (#220) Date conversion methods extras.
out += d.toJSON() + "\n";
out += d.toLocaleString() + "\n";
out += d.toLocaleTimeString() + "\n";
out += d.toTimeString() + "\n";

describe("date_to_json", () => {
  // (cross-runtime #220 + PR #1202) toLocaleString/toLocaleTimeString agora
  // formatam em padrao locale-friendly (DD/MM/YYYY, HH:MM:SS) em vez de
  // ISO, casando com Bun/Node em locale Windows default.
  test("toJSON/toLocaleString/toTimeString (#220)", () => expect(out).toBe(
    "1970-01-01T00:00:00.000Z\n01/01/1970, 00:00:00\n00:00:00\n00:00:00.000Z\n"
  ));
});
