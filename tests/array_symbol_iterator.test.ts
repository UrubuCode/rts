import { describe, test, expect } from "rts:test";

// (#216/299) `arr[Symbol.iterator]` resolve p/ um handle Function nativo
// (ARRAY_VALUES_ITER) — typeof === "function", chamavel. Antes a key
// ausente no Vec dava 0 -> typeof "number". Reusa ITERATOR_FROM.
const a = [1, 2, 3];
const iterType: string = typeof a[Symbol.iterator];

// arguments tambem (array-like) — espelha o fixture 299.
function f(): string {
  return typeof (arguments as any)[Symbol.iterator];
}
const argsIterType = f(1, 2, 3);

// chave symbol custom (nao-iterator) ausente continua undefined-ish.
const s = Symbol("x");
const sType: string = typeof a[s];

describe("array_symbol_iterator (#299)", () => {
  test("arr[Symbol.iterator] eh function", () => expect(iterType).toBe("function"));
  test("arguments[Symbol.iterator] eh function", () => expect(argsIterType).toBe("function"));
  test("symbol custom ausente nao eh function", () => expect(sType === "function").toBe(false));
});
