import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

const nums = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

// filter
const evens = nums.filter(x => x % 2 === 0);
print(evens.join(","));

// find / findIndex
print(`${nums.find(x => x > 5)}`);
print(`${nums.findIndex(x => x > 5)}`);

// some / every
print(`${nums.some(x => x > 9)}`);
print(`${nums.every(x => x > 0)}`);
print(`${nums.every(x => x > 5)}`);

// flat
const nested = [[1, 2], [3, [4, 5]]];
print(nested.flat().join(","));

// Array.from
const fromLen = Array.from({ length: 4 }, (_, i) => i * 3);
print(fromLen.join(","));

// Array.isArray
print(`${Array.isArray([1, 2])}`);
print(`${Array.isArray("not array")}`);

// slice (non-mutating)
const sliced = nums.slice(2, 5);
print(sliced.join(","));

// splice (mutating)
const arr = [1, 2, 3, 4, 5];
const removed = arr.splice(1, 2, 10, 20);
print(arr.join(","));
print(removed.join(","));

// indexOf / lastIndexOf
const arr2 = [1, 2, 3, 2, 1];
print(`${arr2.indexOf(2)}`);
print(`${arr2.lastIndexOf(2)}`);
print(`${arr2.indexOf(99)}`);

describe("array_native_methods", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe(
    "2,4,6,8,10\n6\n5\ntrue\ntrue\nfalse\n1,2,3,4,5\n0,3,6,9\ntrue\nfalse\n3,4,5\n1,10,20,4,5\n2,3\n1\n3\n-1\n"
  ));
});
