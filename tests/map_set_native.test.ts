import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Map
const m = new Map<string, number>();
m.set("a", 1);
m.set("b", 2);
m.set("c", 3);
print(`${m.size}`);
print(`${m.get("b")}`);
print(`${m.has("c")}`);
print(`${m.has("z")}`);
m.delete("b");
print(`${m.size}`);

// Map iteration
for (const [k, v] of m) {
  print(`${k}:${v}`);
}

// Map from entries
const m2 = new Map([["x", 10], ["y", 20]]);
print(`${m2.get("x")} ${m2.get("y")}`);

// Set
const s = new Set<number>([1, 2, 2, 3, 3, 4]);
print(`${s.size}`);
print(`${s.has(2)}`);
print(`${s.has(99)}`);
s.add(5);
s.delete(1);
print(`${s.size}`);

// Set iteration
const arr: number[] = [];
for (const v of s) arr.push(v);
print(arr.join(","));

// WeakMap (basic)
const wm = new WeakMap();
const key = {};
wm.set(key, "value");
print(`${wm.has(key)}`);
print(`${wm.get(key)}`);

describe("map_set_native", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe(
    "3\n2\ntrue\nfalse\n2\na:1\nc:3\n10 20\n4\ntrue\nfalse\n4\n2,3,4,5\ntrue\nvalue\n"
  ));
});
