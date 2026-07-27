import { describe, test, expect } from "rts:test";

// Regression: a WRAPPER OBJECT of a Rust value-class (`new String(x)` /
// `new Boolean(x)` / `new Number(x)`) must run through JS ToPrimitive
// (toString/valueOf) the same way a plain user-class instance does.
// Before the fix, `String`/`Boolean` (migrated to `#[rtse::class(.., value)]`)
// fell through to the default `"[object Object]"` because their instance
// class was tracked only via `global_instance_class`, which the ToPrimitive
// path (`toprimitive.rs`) never consulted — only `static_instance_class`
// (user `.ts` classes) was checked.

const s = new String("z");
const b1 = new Boolean(false);
const b2 = new Boolean(true);
const n = new Number(5);

describe("wrapper value-class ToPrimitive", () => {
  // String(w)
  test("String(new String)", () => expect(String(s)).toBe("z"));
  test("String(new Boolean false)", () => expect(String(b1)).toBe("false"));
  test("String(new Boolean true)", () => expect(String(b2)).toBe("true"));
  test("String(new Number)", () => expect(String(n)).toBe("5"));

  // template literal `${w}`
  test("template String wrapper", () => expect(`${s}`).toBe("z"));
  test("template Boolean wrapper false", () => expect(`${b1}`).toBe("false"));
  test("template Boolean wrapper true", () => expect(`${b2}`).toBe("true"));
  test("template Number wrapper", () => expect(`${n}`).toBe("5"));

  // `w + ""`
  test("String wrapper + empty string", () => expect(s + "").toBe("z"));
  test("Boolean wrapper false + empty string", () => expect(b1 + "").toBe("false"));
  test("Boolean wrapper true + empty string", () => expect(b2 + "").toBe("true"));
  test("Number wrapper + empty string", () => expect(n + "").toBe("5"));

  // direct .toString() / .valueOf()
  test("String wrapper .toString()", () => expect(s.toString()).toBe("z"));
  test("Boolean wrapper .toString()", () => expect(b1.toString()).toBe("false"));
  test("Number wrapper .toString()", () => expect(n.toString()).toBe("5"));
  test("String wrapper .valueOf()", () => expect(s.valueOf()).toBe("z"));
  test("Boolean wrapper .valueOf()", () => expect(b1.valueOf()).toBe(false));
  test("Number wrapper .valueOf()", () => expect(n.valueOf()).toBe(5));

  // inline `new` (not bound to a local first) exercises the `HirExprKind::New`
  // branch of `static_instance_class`/`global_instance_class` directly.
  test("String(new String inline)", () => expect(String(new String("q"))).toBe("q"));
  test("String(new Boolean inline)", () => expect(String(new Boolean(false))).toBe("false"));
  test("template new Boolean inline", () => expect(`${new Boolean(true)}`).toBe("true"));
  test("new Boolean inline + empty string", () => expect(new Boolean(false) + "").toBe("false"));
});
