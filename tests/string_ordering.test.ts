import { describe, test, expect } from "rts:test";

// Comparacoes lexicograficas string<string (#616)
const a_ge_b = "5" >= "0";
const a_le_b = "0" <= "5";
const a_gt_b = "b" > "a";
const a_lt_b = "a" < "b";
const eq_eq = "abc" >= "abc";
const eq_le = "abc" <= "abc";
const prefix_lt = "ab" < "abc";
const prefix_gt = "abc" > "ab";
const empty_lt = "" < "a";
const dig_in_range = ("5" >= "0") && ("5" <= "9");
const alpha_in_range = ("c" >= "a") && ("c" <= "z");

describe("string ordering #616", () => {
  test("'5' >= '0'", () => expect(a_ge_b).toBe(true));
  test("'0' <= '5'", () => expect(a_le_b).toBe(true));
  test("'b' > 'a'", () => expect(a_gt_b).toBe(true));
  test("'a' < 'b'", () => expect(a_lt_b).toBe(true));
  test("'abc' >= 'abc'", () => expect(eq_eq).toBe(true));
  test("'abc' <= 'abc'", () => expect(eq_le).toBe(true));
  test("prefix less", () => expect(prefix_lt).toBe(true));
  test("prefix greater", () => expect(prefix_gt).toBe(true));
  test("empty less", () => expect(empty_lt).toBe(true));
  test("digit in range '0'..'9'", () => expect(dig_in_range).toBe(true));
  test("alpha in range 'a'..'z'", () => expect(alpha_in_range).toBe(true));
});
