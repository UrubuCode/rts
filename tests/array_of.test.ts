import { describe, test, expect } from "rts:test";

let out = "";

// (#208) Array.of(...args) — cria Vec com cada arg.
const a = Array.of(1, 2, 3);
out += a.join(",") + "\n";  // 1,2,3

const b = Array.of(42);
out += b.join(",") + "\n";  // 42 (1 elemento, nao [length: 42])

const empty = Array.of();
out += empty.length + "\n"; // 0

describe("array_of", () => {
  test("Array.of variadico (#208)", () => expect(out).toBe(
    "1,2,3\n42\n0\n"
  ));
});
