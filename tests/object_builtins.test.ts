import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

const obj = { a: 1, b: 2, c: 3 };

// Object.keys
print(Object.keys(obj).join(","));

// Object.values
print(Object.values(obj).join(","));

// Object.entries
Object.entries(obj).forEach(([k, v]) => print(`${k}=${v}`));

// Object.assign
const target = { x: 1 };
const result = Object.assign(target, { y: 2 }, { z: 3 });
print(`${result.x} ${result.y} ${result.z}`);

// Object.freeze
const frozen = Object.freeze({ n: 42 });
print(`${frozen.n}`);
// attempting mutation should be no-op in non-strict
frozen.n = 99;
print(`${frozen.n}`);

// Object.fromEntries
const entries: [string, number][] = [["a", 1], ["b", 2], ["c", 3]];
const fromEnt = Object.fromEntries(entries);
print(`${fromEnt.a} ${fromEnt.b} ${fromEnt.c}`);

// Object.hasOwn (ES2022)
print(`${Object.hasOwn(obj, "a")}`);
print(`${Object.hasOwn(obj, "toString")}`);

describe("object_builtins", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe(
    "a,b,c\n1,2,3\na=1\nb=2\nc=3\n1 2 3\n42\n42\n1 2 3\ntrue\nfalse\n"
  ));
});
