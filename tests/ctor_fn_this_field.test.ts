import { describe, test, expect } from "rts:test";

// (#601) this.name em ctor function — nao deve ser interceptado por
// Function.name getter.

function Animal(n: string) {
  (this as any).name = n;
  (this as any).age = 5;
}
const a = new (Animal as any)("Rex");
const a_name = (a as any).name;
const a_age = (a as any).age;

describe("ctor_fn_this_field", () => {
  test("this.name = n persiste", () => expect(a_name).toBe("Rex"));
  test("this.age = 5 persiste", () => expect(a_age).toBe(5));
});
