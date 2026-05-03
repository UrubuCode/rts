import { describe, test, expect } from "rts:test";

let out = "";
function print(v: string): void { out += v + "\n"; }

const obj: { [k: string]: number } = { a: 1, b: 2, c: 3 };

print(Reflect.get(obj, "a").toString());

print(Reflect.has(obj, "a") ? "yes" : "no");
print(Reflect.has(obj, "z") ? "yes" : "no");

Reflect.set(obj, "d", 4);
print(Reflect.get(obj, "d").toString());

Reflect.deleteProperty(obj, "a");
print(Reflect.has(obj, "a") ? "yes" : "no");

const keys = Reflect.ownKeys(obj);
print(keys.join(","));

describe("reflect_v0", () => {
  test("acceptance #218 v0", () => expect(out).toBe(
    "1\nyes\nno\n4\nno\nb,c,d\n"
  ));
});
