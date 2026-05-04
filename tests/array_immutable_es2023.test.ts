import { describe, test, expect } from "rts:test";

let out = "";

// (#208 ES2023) Array immutable variants — não mutam o original.

const a = [3, 1, 2];
const sorted: number[] = a.toSorted();
out += sorted.join(",") + "\n";   // 1,2,3
out += a.join(",") + "\n";        // 3,1,2 (preservado)

const b = [1, 2, 3];
const reversed: number[] = b.toReversed();
out += reversed.join(",") + "\n"; // 3,2,1
out += b.join(",") + "\n";        // 1,2,3

const c = [1, 2, 3, 4, 5];
const spliced: number[] = c.toSpliced(1, 2);
out += spliced.join(",") + "\n";  // 1,4,5
out += c.join(",") + "\n";        // 1,2,3,4,5

const d = [10, 20, 30];
const replaced: number[] = d.with(1, 99);
out += replaced.join(",") + "\n"; // 10,99,30
out += d.join(",") + "\n";        // 10,20,30

describe("array_immutable_es2023", () => {
  test("toSorted/toReversed/toSpliced/with (#208)", () => expect(out).toBe(
    "1,2,3\n3,1,2\n3,2,1\n1,2,3\n1,4,5\n1,2,3,4,5\n10,99,30\n10,20,30\n"
  ));
});
