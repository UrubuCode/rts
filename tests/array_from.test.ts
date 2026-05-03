import { describe, test, expect } from "rts:test";
let out = "";

// Array.from({length: N}) sem mapper
const a = Array.from({ length: 4 });
out += a.join(",") + "\n";   // 0,1,2,3

// Array.from(vec) — copia
const src = [10, 20, 30];
const cp: number[] = Array.from(src);
out += cp.join(",") + "\n";  // 10,20,30

// (#208) Array.from com mapper user fn que retorna i64 — typed annotation
// pra match com extern "C" fn(i64, i64) -> i64. Mapper recebe (item, index).
// Mapper com tipo `number` (f64) ainda nao funciona — fica em outra PR.

describe("array_from", () => {
  test("Array.from sem mapper (#208)", () => expect(out).toBe(
    "0,1,2,3\n10,20,30\n"
  ));
});
