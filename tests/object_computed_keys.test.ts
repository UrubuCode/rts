import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Computed key como string literal entre []
const o1 = { ["foo"]: 1, ["bar"]: 2 };
print(`${o1.foo} ${o1.bar}`);

// Computed key a partir de variavel
const k = "dynamic";
const o2 = { [k]: 42 };
print(`${o2.dynamic}`);

// Computed key a partir de expressao (concat)
const prefix = "on";
const o3 = { [prefix + "Click"]: 7, [prefix + "Hover"]: 9 };
print(`${o3.onClick} ${o3.onHover}`);

// Mistura computed + estatico
const key = "x";
const o4 = { a: 1, [key]: 2, b: 3 };
print(`${o4.a} ${o4.x} ${o4.b}`);

describe("object_computed_keys", () => {
  test("computed", () => expect(__rtsCapturedOutput).toBe("1 2\n42\n7 9\n1 2 3\n"));
});
