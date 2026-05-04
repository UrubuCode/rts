import { describe, test, expect } from "rts:test";

// typeof <class> = "function" (JS spec). Cobre user e global classes.

class MyClass {}

const a = typeof MyClass;
const b = typeof Number;
const c = typeof String;
const d = typeof Date;
const e = typeof Error;

describe("typeof_class", () => {
  test("typeof user class", () => expect(a).toBe("function"));
  test("typeof Number", () => expect(b).toBe("function"));
  test("typeof String", () => expect(c).toBe("function"));
  test("typeof Date", () => expect(d).toBe("function"));
  test("typeof Error", () => expect(e).toBe("function"));
});
