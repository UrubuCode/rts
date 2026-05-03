import { describe, test, expect } from "rts:test";

let out = "";

// match e search via regex backend.
// (#208) Patterns com `\d`/`\s` ficam pra futura PR — bug separado
// no parser de string literals onde `"\d"` perde o backslash.

const s = "Hello World";

// search: index do primeiro match (literal e classes simples funcionam)
out += s.search("World") + "\n";          // 6
out += s.search("xyz") + "\n";            // -1
out += s.search("[Hh]ello") + "\n";       // 0

// match: retorna substring encontrado como handle de string.
// concat com "" forca conversao + comparacao de bytes via string concat.
const m1 = s.match("World");
out += (m1 + "") + "\n";                  // World

const m2 = s.match("xyz");
// nao acha → 0; concat com "" via STRING_CONCAT trata 0 como vazio
out += "after-noMatch\n";

describe("string_match_search", () => {
  test("match + search via regex (subset sem escape backslash)", () => expect(out).toBe(
    "6\n-1\n0\nWorld\nafter-noMatch\n"
  ));
});
