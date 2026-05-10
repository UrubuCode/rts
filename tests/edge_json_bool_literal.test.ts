// Cobertura do fix de JSON.stringify para object/array literais com
// bool/null. Antes: `{x: true}` virava `{"x":1}` porque Map<String,i64>
// nao distinguia bool/null de numero. Fix: try_const_stringify gera
// JSON em compile time quando arg eh literal AST puro (Lit/Object/Array).
import { describe, test, expect } from "rts:test";

describe("JSON.stringify literais com bool/null/nested (#json-bool)", () => {
  test("bool true em obj", () => expect(JSON.stringify({ x: true })).toBe('{"x":true}'));
  test("bool false em obj", () => expect(JSON.stringify({ y: false })).toBe('{"y":false}'));
  test("null em obj", () => expect(JSON.stringify({ z: null })).toBe('{"z":null}'));
  test("nested bool", () => expect(JSON.stringify({ nested: { a: true } })).toBe('{"nested":{"a":true}}'));
  test("array com bool", () => expect(JSON.stringify([true, false, 1])).toBe("[true,false,1]"));
  test("array com null", () => expect(JSON.stringify([1, null, 2])).toBe("[1,null,2]"));
  test("array com string", () => expect(JSON.stringify(["a", "b"])).toBe('["a","b"]'));
  test("nested obj+array", () => expect(JSON.stringify({ a: [1, 2], b: { c: false } })).toBe('{"a":[1,2],"b":{"c":false}}'));
  test("primitivo bool", () => expect(JSON.stringify(true)).toBe("true"));
  test("primitivo null", () => expect(JSON.stringify(null)).toBe("null"));
  test("primitivo num", () => expect(JSON.stringify(42)).toBe("42"));
  test("primitivo string", () => expect(JSON.stringify("hi")).toBe('"hi"'));
  test("string com escape", () => expect(JSON.stringify('"quoted"')).toBe('"\\"quoted\\""'));
  test("obj vazio", () => expect(JSON.stringify({})).toBe("{}"));
  test("array vazio", () => expect(JSON.stringify([])).toBe("[]"));
  test("numero negativo", () => expect(JSON.stringify(-7)).toBe("-7"));
});
