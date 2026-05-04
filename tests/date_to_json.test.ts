import { describe, test, expect } from "rts:test";

let out = "";

const d = new Date(0);

// (#220) Date conversion methods extras.
out += d.toJSON() + "\n";
out += d.toLocaleString() + "\n";
out += d.toLocaleTimeString() + "\n";
out += d.toTimeString() + "\n";

describe("date_to_json", () => {
  test("toJSON/toLocaleString/toTimeString (#220)", () => expect(out).toBe(
    "1970-01-01T00:00:00.000Z\n1970-01-01T00:00:00.000Z\n00:00:00.000Z\n00:00:00.000Z\n"
  ));
});
