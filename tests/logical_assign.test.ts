import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Nullish coalescing assignment ??=
let a: number | null = null;
a ??= 42;
print(`${a}`);

let b: number | null = 10;
b ??= 99;
print(`${b}`);

// Logical OR assignment ||=
let c = 0;
c ||= 5;
print(`${c}`);

let d = 3;
d ||= 99;
print(`${d}`);

// Logical AND assignment &&=
let e = 1;
e &&= 2;
print(`${e}`);

let f = 0;
f &&= 99;
print(`${f}`);

// On object properties
const obj = { x: 0, y: 1, z: null as number | null };
obj.x ||= 7;
obj.y &&= 8;
obj.z ??= 9;
print(`${obj.x} ${obj.y} ${obj.z}`);

describe("logical_assign", () => {
  test("nullish", () => expect(__rtsCapturedOutput).toBe("42\n10\n5\n3\n2\n0\n7 8 9\n"));
});
