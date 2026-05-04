import { describe, test, expect } from "rts:test";

// Date.prototype.toString — formato JS spec, nao ISO.

const r = new Date(0).toString();

describe("date_to_string_jsspec", () => {
  test("epoch", () => expect(r).toBe(
    "Thu Jan 01 1970 00:00:00 GMT+0000 (Coordinated Universal Time)"
  ));
});
