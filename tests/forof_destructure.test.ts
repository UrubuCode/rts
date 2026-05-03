import { describe, test, expect } from "rts:test";

let out = "";

// (#210) for-of com array destructuring direto em Object.entries
const obj = { a: 1, b: 2, c: 3 };
for (const [k, v] of Object.entries(obj)) {
  out += k + "=" + v + "\n";
}

// for-of com tuplas explicitas (slot 0 default I64)
const pairs: [number, number][] = [[10, 20], [30, 40]];
for (const [x, y] of pairs) {
  out += x + "+" + y + "\n";
}

describe("forof_destructure", () => {
  test("entries + tuples", () => expect(out).toBe(
    "a=1\nb=2\nc=3\n10+20\n30+40\n"
  ));
});
