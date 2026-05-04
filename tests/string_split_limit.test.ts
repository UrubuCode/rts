import { describe, test, expect } from "rts:test";

let out = "";

// (#208) String.split(sep, limit?) — limit truncamento.
const a: string[] = "a,b,c,d".split(",", 2);
out += a.length + ":" + a.join("|") + "\n";   // 2:a|b

// Sem limit ainda funciona
const b: string[] = "x:y:z".split(":");
out += b.length + ":" + b.join(",") + "\n";   // 3:x,y,z

// Limit 0 = vazio
const c: string[] = "a,b,c".split(",", 0);
out += c.length + "\n";                       // 0

describe("string_split_limit", () => {
  test("split com limit (#208)", () => expect(out).toBe(
    "2:a|b\n3:x,y,z\n0\n"
  ));
});
