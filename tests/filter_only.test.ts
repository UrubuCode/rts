import { describe, test, expect } from "rts:test";
let out: string = "";
function print(v: string): void { out += v + "\n"; }
const nums = [1,2,3,4,5];
const evens = nums.filter(x => x % 2 === 0);
print(evens.join(","));
describe("f", () => { test("t", () => expect(out).toBe("2,4\n")); });
