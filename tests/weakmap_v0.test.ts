import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const obj1 = {};
const obj2 = {};
const wm = new WeakMap();
wm.set(obj1, 42);
const v = wm.get(obj1);
print(`${v}`);
print(wm.has(obj1) ? "yes" : "no");
print(wm.has(obj2) ? "yes" : "no");
wm.delete(obj1);
print(wm.has(obj1) ? "yes" : "no");

describe("weakmap_v0", () => {
  test("acceptance #217 v0", () => expect(out).toBe("42\nyes\nno\nno\n"));
});
