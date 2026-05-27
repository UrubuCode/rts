import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `const parts = s.split(sep); parts.map(...)`
// crashava SIGILL. collect_array_receiver_idents nao reconhecia `split` como
// array-returning, entao a var `parts` nao era registrada e `parts.map(...)`
// nao era roteado como array method -> crash. (split em chain direto ja'
// funcionava — estava em looks_array_call mas faltava no collect.)

let out = "";
function print(v: string): void { out += v + "\n"; }

const parts = "a-b-c".split("-");
print(parts.map(p => p + "!").join(""));      // a!b!c!

const nums = "1,2,3".split(",");
print(nums.filter(n => n !== "2").join(""));  // 13

const words = "hello world foo".split(" ");
print(words.map(w => w.length).join(","));    // 5,5,3

// chain direto continua OK
print("x.y.z".split(".").map(s => s.toUpperCase()).join("")); // XYZ

describe("string split var map", () => {
  test("var de split eh reconhecida como array", () =>
    expect(out).toBe("a!b!c!\n13\n5,5,3\nXYZ\n"));
});
