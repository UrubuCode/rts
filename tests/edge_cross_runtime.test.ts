// Suite de paridade cross-runtime: outputs RTS, Bun e Node devem
// bater. Foi validado manualmente em 2026-05-10. Re-rodar quando
// adicionar novos casos:
//   bun tests/edge_cross_runtime.test.ts (adaptado: console.log)
//   node tests/edge_cross_runtime.test.ts (adaptado)
//   rts.exe test tests/edge_cross_runtime.test.ts
import { describe, test, expect } from "rts:test";

// === Logical operators com strings ===
const lo1: string = "" || "" || "fb" || "ignored";
const lo2: string = "first" || "second" || "third";
const lo3: string = "" && "x" || "fallback";
const lo4: string = "a" || "b" && "c";  // precedencia: 'a' || ('b' && 'c') = 'a'
const lo5: string = "" || "x" && "y";   // precedencia: '' || ('x' && 'y') = 'y'
const lo6: string = "   " || "fb";       // whitespace eh truthy

// === Switch com strings ===
function color(s: string): number {
  switch (s) {
    case "red": return 1;
    case "green": return 2;
    default: return 0;
  }
}

function priority(s: string): string {
  switch (s) {
    case "high":
    case "urgent": return "important";
    default: return "normal";
  }
}

// === Truthiness em ternario ===
const empty: string = "";
const filled: string = "value";
const tt1: string = empty ? "yes" : "no";
const tt2: string = filled ? "yes" : "no";

describe("paridade RTS/Bun/Node — logical + switch + truthy", () => {
  test("'' || '' || 'fb' = 'fb'", () => expect(lo1).toBe("fb"));
  test("'first' || ... = 'first'", () => expect(lo2).toBe("first"));
  test("'' && 'x' || 'fb' = 'fb'", () => expect(lo3).toBe("fallback"));
  test("'a' || 'b' && 'c' = 'a'", () => expect(lo4).toBe("a"));
  test("'' || 'x' && 'y' = 'y'", () => expect(lo5).toBe("y"));
  test("'   ' truthy", () => expect(lo6).toBe("   "));
  test("switch red", () => expect(color("red")).toBe(1));
  test("switch unknown", () => expect(color("xx")).toBe(0));
  test("switch fallthrough", () => expect(priority("urgent")).toBe("important"));
  test("ternary empty falsy", () => expect(tt1).toBe("no"));
  test("ternary filled truthy", () => expect(tt2).toBe("yes"));
});
