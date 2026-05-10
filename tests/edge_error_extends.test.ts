// Cobertura do fix de `class X extends Error` — super(msg) agora
// armazena message+name no map de this. Antes: super para Error class
// caia em fallback que descartava args -> e.message="" -> "null" no template.
import { describe, test, expect } from "rts:test";

class A extends Error {
  constructor(m: string) { super(m); }
}

class B extends Error {
  constructor(public code: number, m: string) {
    super(m);
    this.name = "B";
  }
}

class CustomTypeError extends TypeError {
  constructor(m: string) { super(m); }
}

const a = new A("hello");
const b = new B(42, "with code");
const c = new CustomTypeError("type oops");

let caughtMsg = "";
let caughtCode = 0;
let caughtName = "";

try {
  throw new A("inside try");
} catch (e: any) {
  caughtMsg = e.message;
}

try {
  throw new B(99, "rich");
} catch (e: any) {
  caughtCode = e.code;
  caughtName = e.name;
}

let chained = "";
try {
  try {
    throw new TypeError("inner");
  } catch (e: any) {
    throw new RangeError("re: " + e.message);
  }
} catch (e: any) {
  chained = e.message;
}

describe("class extends Error — super propaga message (#err-extends)", () => {
  test("A.message", () => expect(a.message).toBe("hello"));
  test("B.message", () => expect(b.message).toBe("with code"));
  test("B.code (custom field)", () => expect(b.code).toBe(42));
  test("B.name override", () => expect(b.name).toBe("B"));
  test("CustomTypeError.message", () => expect(c.message).toBe("type oops"));
  test("caught A.message", () => expect(caughtMsg).toBe("inside try"));
  test("caught B.code", () => expect(caughtCode).toBe(99));
  test("caught B.name", () => expect(caughtName).toBe("B"));
  test("chained throw rethrow", () => expect(chained).toBe("re: inner"));
});
