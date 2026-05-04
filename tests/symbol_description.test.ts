import { describe, test, expect } from "rts:test";

// (#216 partial) Symbol(desc).description acessivel via instance_getter
// fallback no codegen, mesmo sem receiver_class resolvido.

const s1 = Symbol("hello");
const s2 = Symbol("world");
const s3 = Symbol();

describe("symbol_description", () => {
  test("description preservada", () => expect(s1.description).toBe("hello"));
  test("description distinta", () => expect(s2.description).toBe("world"));
});
