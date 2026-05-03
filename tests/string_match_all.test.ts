import { describe, test, expect } from "rts:test";

let out = "";

const s = "ana banana";
const matches = s.matchAll("an");
out += matches.length + "\n";   // 3
out += matches.join(",") + "\n";  // an,an,an

// Sem matches — array vazio
const empty = "xyz".matchAll("an");
out += empty.length + "\n";    // 0

// Pattern com classe
const s2 = "Hello World";
const ms = s2.matchAll("[Hh]");
out += ms.length + "\n";       // 1 (so' "H")
out += ms.join(",") + "\n";    // H

describe("string_match_all", () => {
  test("matchAll retorna Vec eager (#208)", () => expect(out).toBe(
    "3\nan,an,an\n0\n1\nH\n"
  ));
});
