import { describe, test, expect } from "rts:test";

// (#550) ! operator com strings: !"" === true; !"x" === false.
// Tambem cobre handle 0 (regexp.exec sem match).

const a = !"";
const b = !"x";
const c = !!"";
const d = !!"x";
const e = !NaN;
const f = !0;
const g = !1;

describe("bang_operator_strings", () => {
  test("!'' === true", () => expect(a).toBe(true));
  test("!'x' === false", () => expect(b).toBe(false));
  test("!!'' === false", () => expect(c).toBe(false));
  test("!!'x' === true", () => expect(d).toBe(true));
  test("!NaN === true", () => expect(e).toBe(true));
  test("!0 === true", () => expect(f).toBe(true));
  test("!1 === false", () => expect(g).toBe(false));
});
