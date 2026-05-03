import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }
function printNum(n: number): void { out += n + "\n"; }

const nums = [1,2,3,4,5,6,7,8,9,10];

const evens: number[] = nums.filter(x => x % 2 === 0);
print(evens.join(","));

const big = nums.find(x => x > 5);
printNum(big);

const idx = nums.findIndex(x => x > 5);
printNum(idx);

const some_big: boolean = nums.some(x => x > 9);
const some_huge: boolean = nums.some(x => x > 99);
const all_pos: boolean = nums.every(x => x > 0);
const all_big: boolean = nums.every(x => x > 5);
print(some_big ? "true" : "false");
print(some_huge ? "true" : "false");
print(all_pos ? "true" : "false");
print(all_big ? "true" : "false");

describe("array_predicate_methods", () => {
  test("all", () => expect(out).toBe(
    "2,4,6,8,10\n6\n5\ntrue\nfalse\ntrue\nfalse\n"
  ));
});
