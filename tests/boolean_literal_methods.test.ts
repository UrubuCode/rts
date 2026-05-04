import { describe, test, expect } from "rts:test";

// (#550 parte 2) (true).toString() / (false).valueOf() em literal.

const a = (true).toString();
const b = (false).toString();
const c = (true).valueOf();
const d = (false).valueOf();

describe("boolean_literal_methods", () => {
  test("(true).toString() (#550)", () => expect(a).toBe("true"));
  test("(false).toString() (#550)", () => expect(b).toBe("false"));
  test("(true).valueOf() (#550)", () => expect(c).toBe(true));
  test("(false).valueOf() (#550)", () => expect(d).toBe(false));
});
