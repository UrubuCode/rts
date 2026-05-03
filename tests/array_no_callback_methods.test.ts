import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const a = [1,2,3,4,5];
print(`${a.indexOf(3)}`);
print(`${a.lastIndexOf(3)}`);
print(`${a.includes(99)}`);
print(`${a.includes(2)}`);

const sliced: number[] = a.slice(1, 3);
print(sliced.join(","));

const sl_neg: number[] = a.slice(-2);
print(sl_neg.join(","));

const concat: number[] = a.concat([6, 7]);
print(concat.join(","));

print(`${Array.isArray(a)}`);

const b = [1,2,3];
b.reverse();
print(b.join(","));

const c = [1,2,3,4,5];
const removed: number[] = c.splice(1, 2);
print(c.join(","));
print(removed.join(","));

describe("array_no_callback_methods", () => {
  test("all", () => expect(out).toBe(
    "2\n2\nfalse\ntrue\n2,3\n4,5\n1,2,3,4,5,6,7\ntrue\n3,2,1\n1,4,5\n2,3\n"
  ));
});
