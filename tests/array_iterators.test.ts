import { describe, test, expect } from "rts:test";

let out = "";

const arr = [10, 20, 30];

// (#208) values() — Vec eager copia
const vs = arr.values();
out += vs.join(",") + "\n";   // 10,20,30

// keys() — indices
const ks = arr.keys();
out += ks.join(",") + "\n";   // 0,1,2

// entries() — pares [idx, value]
const entries = arr.entries();
out += entries.length + "\n"; // 3

// for-of destructuring em entries (sinergia com PR #494)
for (const [i, v] of arr.entries()) {
  out += i + "=" + v + "\n";
}

describe("array_iterators", () => {
  test("values/keys/entries (#208 eager v0)", () => expect(out).toBe(
    "10,20,30\n0,1,2\n3\n0=10\n1=20\n2=30\n"
  ));
});
