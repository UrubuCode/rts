import { describe, test, expect } from "rts:test";

let out = "";

// (#210) Acceptance criteria — todos os 5 cenarios da issue.

// 1. Nested array destructuring
const [[a, b]] = [[1, 2]];
out += a + "," + b + "\n";

// 2. Nested object destructuring (#498)
const { x: { y } } = { x: { y: 42 } };
out += y + "\n";

// 3. Rest em array (já funcionava)
const [first, ...rest]: number[] = [1, 2, 3, 4];
out += first + "/" + rest.length + "\n";

// 4. Rest em object (já funcionava)
const { a: aa, ...others } = { a: 1, b: 2, c: 3 };
out += aa + "/" + Object.keys(others as any).length + "\n";

// 5. Default values em object (já funcionavam)
const { p = 99, q = 88 } = { p: 1 } as { p?: number; q?: number };
out += p + "," + q + "\n";

// 6. Parameter destructuring (já funcionava)
function fn({ x, y }: { x: number; y: number }): number { return x + y; }
out += fn({ x: 10, y: 20 }) + "\n";

// 7. for-of com destructuring (#494)
const pairs: [number, number][] = [[1, 2], [3, 4]];
for (const [u, v] of pairs) { out += u + "+" + v + " "; }
out += "\n";

describe("destructure_acceptance_210", () => {
  test("acceptance criteria #210 (todos)", () => expect(out).toBe(
    "1,2\n42\n1/3\n1/2\n1,88\n30\n1+2 3+4 \n"
  ));
});
