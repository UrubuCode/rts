import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

print("abs=" + URL.canParse("https://example.com/x"));
print("rel=" + URL.canParse("/x", "https://example.com"));
print("ftp=" + URL.canParse("ftp://files.example.com/a"));
// Sem scheme://: nao parsa
print("noscheme=" + URL.canParse("just a string"));

describe("URL.canParse (#746)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "abs=true\n" +
      "rel=true\n" +
      "ftp=true\n" +
      "noscheme=false\n"
    )
  );
});
