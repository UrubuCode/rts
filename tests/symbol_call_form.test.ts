import { describe, test, expect } from "rts:test";

// (#216 partial) Symbol("desc") chamado como funcao em vez de
// global member resolve para __RTS_FN_GL_SYMBOL_NEW.

const s1 = Symbol("hello");
const s2 = Symbol("world");
const s3 = Symbol();

describe("symbol_call_form", () => {
  test("Symbol(desc) cria handle distinto", () => expect(s1 !== s2).toBe(true));
  test("Symbol() sem desc cria handle distinto", () => expect(s2 !== s3).toBe(true));
  test("Symbol identidade preservada", () => expect(s1 === s1).toBe(true));
});
