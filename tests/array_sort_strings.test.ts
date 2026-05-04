import { describe, test, expect } from "rts:test";

let out = "";

// (#208) sort default detecta string handles e ordena lexicograficamente.
const fruits = ["banana", "apple", "cherry"];
fruits.sort();
out += fruits.join(",") + "\n";   // apple,banana,cherry

// Numeros continuam funcionando (sort numerico).
const nums = [3, 1, 2];
nums.sort();
out += nums.join(",") + "\n";     // 1,2,3

// Vazio.
const empty: string[] = [];
empty.sort();
out += empty.length + "\n";       // 0

describe("array_sort_strings", () => {
  test("sort default detecta strings vs numbers (#208)", () => expect(out).toBe(
    "apple,banana,cherry\n1,2,3\n0\n"
  ));
});
