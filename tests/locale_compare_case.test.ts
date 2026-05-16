import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// JS spec (ICU/CLDR locale order): quando case-insensitive iguais,
// lowercase vem ANTES de uppercase (oposto do ASCII puro):
//   "A".localeCompare("a") =  1  ("a" vem antes)
//   "a".localeCompare("A") = -1
// Bate com Bun/Node — antes RTS retornava o oposto (#156).
print("A_a=" + "A".localeCompare("a"));
print("a_A=" + "a".localeCompare("A"));
print("A_A=" + "A".localeCompare("A"));

// Case-insensitive diff
print("a_b=" + ("a".localeCompare("b") < 0));
print("B_a=" + "B".localeCompare("a")); // case-insensitive: b > a, returns 1
print("a_B=" + "a".localeCompare("B")); // case-insensitive: a < b, returns -1

describe("String.localeCompare case order (#156)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "A_a=1\n" +
      "a_A=-1\n" +
      "A_A=0\n" +
      "a_b=true\n" +
      "B_a=1\n" +
      "a_B=-1\n"
    )
  );
});
