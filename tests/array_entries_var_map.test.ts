import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `const e = arr.entries(); e.map(...)` crashava
// SIGILL. collect_array_receiver_idents nao reconhecia entries/keys/values
// como array-returning, entao a var nao era registrada e `.map(...)` sobre
// ela nao era roteado como array method. Fix: adiciona-los a lista do collect.

let out = "";
function print(v: string): void { out += v + "\n"; }

const arr = [10, 20, 30];

const e = arr.entries();
print(e.map((p: [number, number]) => p[0] + ":" + p[1]).join(",")); // 0:10,1:20,2:30

const k = arr.keys();
print(k.map(i => i * 2).join(","));   // 0,2,4

const v = arr.values();
print(v.filter(x => x > 15).join(",")); // 20,30

describe("array entries var map", () => {
  test("var de entries/keys/values eh array", () =>
    expect(out).toBe("0:10,1:20,2:30\n0,2,4\n20,30\n"));
});
