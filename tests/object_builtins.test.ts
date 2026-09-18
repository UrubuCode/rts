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
// A module is strict code, so writing a frozen property THROWS a TypeError
// here (Node 20 and Bun 1.4 both do, on this same file); it is only sloppy
// script code where the write fails silently. Either way the value stays.
try {
  frozen.n = 99;
  print("no throw");
} catch (e) {
  print(`${e instanceof TypeError}`);
}
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
    "a,b,c\n1,2,3\na=1\nb=2\nc=3\n1 2 3\n42\ntrue\n42\n1 2 3\ntrue\nfalse\n"
  ));
});
