import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const m = new Map();
m.set("a", 1);
m.set("b", 2);
m.set("c", 3);

const collected: string[] = [];
m.forEach((value: number, key: string) => {
  collected.push(key + "=" + value);
});
print("size=" + m.size);
print("forEach=" + collected.join(","));

// Map([[k,v]]) constructor
const m2 = new Map([["x", 10], ["y", 20]]);
const lines: string[] = [];
m2.forEach((value: number, key: string) => {
  lines.push(key + ":" + value);
});
print("ctor=" + lines.join(","));

describe("Map.forEach (#793)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "size=3\n" +
      "forEach=a=1,b=2,c=3\n" +
      "ctor=x:10,y:20\n"
    )
  );
});
