// Cobertura do fix #627 — destructuring de param em fn user com string
// nao perde tipo em concat. Antes: `function f({x}: {x: string})` com
// `return "Hi, " + x;` emitia STRING_FROM_I64(handle) -> "Hi, 281474976...".
// Fix: var ambigua (init de obj.x sem tipo) marca local_ambiguous_vars,
// cada read propaga var_member_call_values, lower_add usa TPL_COERCE_AUTO.
import { describe, test, expect } from "rts:test";

let __out: string = "";
function out(s: string): void { __out += s + "\n"; }

// Caso minimal
function greet({ x }: { x: string }): string {
  return "Hi, " + x;
}

// Caso com default + 2 fields
function greet2({ name, greeting = "Hi" }: { name: string; greeting?: string }): string {
  return greeting + ", " + name;
}

// Caso com numeric e string mistos
function summarize({ name, age }: { name: string; age: number }): string {
  return name + " (" + age + ")";
}

// Aninhado em param
function greetNested({ user: { name } }: { user: { name: string } }): string {
  return "Hello, " + name;
}

out(greet({ x: "Bob" }));
out(greet2({ name: "Eve" }));
out(greet2({ name: "Eve", greeting: "Hello" }));
out(summarize({ name: "Carl", age: 42 }));
out(greetNested({ user: { name: "Dora" } }));

const expected =
  "Hi, Bob\n" +
  "Hi, Eve\n" +
  "Hello, Eve\n" +
  "Carl (42)\n" +
  "Hello, Dora\n";

describe("Destructuring de param com string em concat (#627)", () => {
  test("string em concat preserva tipo", () => expect(__out).toBe(expected));
});
