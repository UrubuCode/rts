import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const obj1 = {};
const obj2 = {};
const ws = new WeakSet();
ws.add(obj1);
print(ws.has(obj1) ? "yes" : "no");
print(ws.has(obj2) ? "yes" : "no");
ws.delete(obj1);
print(ws.has(obj1) ? "yes" : "no");
ws.add(obj2);
print(ws.has(obj2) ? "yes" : "no");

describe("weakset_v0", () => {
  test("acceptance #217 v0", () => expect(out).toBe("yes\nno\nno\nyes\n"));
});
