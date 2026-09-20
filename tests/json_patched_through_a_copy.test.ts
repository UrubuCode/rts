// The direct call to `JSON.stringify` is spent only when the whole program
// proves `JSON` never leaves a member base — `primordial::only_a_base`. This
// file is the program that proof must REFUSE: the serialiser is replaced
// through a copy of the object, which a walk looking for writes spelled
// `JSON.x = …` does not see. `Math`'s proof accepts that hole; this one closes
// it, because patching a serialiser is a thing programs do.
//
// Every expectation is what node answers.
import { describe, test, expect } from "rts:test";

const held = JSON;
const original = held.stringify;

describe("a serialiser replaced through a copy is the one that runs", () => {
  test("assignment through the alias", () => {
    held.stringify = (() => "patched") as typeof JSON.stringify;
    expect(JSON.stringify({ a: 1 })).toBe("patched");
    held.stringify = original;
    expect(JSON.stringify({ a: 1 })).toBe('{"a":1}');
  });

  test("defineProperty, which writes no place at all", () => {
    Object.defineProperty(JSON, "parse", { value: () => "defined", configurable: true, writable: true });
    expect(JSON.parse("[1]")).toBe("defined");
  });
});
