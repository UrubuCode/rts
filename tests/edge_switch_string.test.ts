// Cobertura do fix de switch com strings — antes comparava handles
// com icmp (igualdade de bits) -> sempre falso pra strings com mesmo
// conteudo mas handles diferentes. Fix: detecta string discriminant
// e usa STRING_EQ.
import { describe, test, expect } from "rts:test";

function color(s: string): number {
  switch (s) {
    case "red": return 1;
    case "green": return 2;
    case "blue": return 3;
    default: return 0;
  }
}

function priority(s: string): string {
  switch (s) {
    case "high":
    case "urgent":
      return "important";
    case "low":
      return "minor";
    default:
      return "normal";
  }
}

// Switch numerico continua funcionando (regressao guard)
function classify(n: number): string {
  switch (n) {
    case 0: return "zero";
    case 1: return "one";
    case 2:
    case 3: return "two-or-three";
    default: return "other";
  }
}

// Fallthrough com strings
function steps(s: string): string {
  let r = "";
  switch (s) {
    case "a":
      r = r + "a";
      // fall through
    case "b":
      r = r + "b";
      break;
    case "c":
      r = r + "c";
      break;
  }
  return r;
}

describe("switch com strings (#sw-str)", () => {
  test("color red", () => expect(color("red")).toBe(1));
  test("color green", () => expect(color("green")).toBe(2));
  test("color blue", () => expect(color("blue")).toBe(3));
  test("color unknown -> default", () => expect(color("yellow")).toBe(0));
  test("priority high (fallthrough case)", () => expect(priority("high")).toBe("important"));
  test("priority urgent (fallthrough case 2)", () => expect(priority("urgent")).toBe("important"));
  test("priority low", () => expect(priority("low")).toBe("minor"));
  test("priority default", () => expect(priority("med")).toBe("normal"));
  // Regressao guard — switch numerico
  test("classify 0", () => expect(classify(0)).toBe("zero"));
  test("classify 2 (fallthrough cases)", () => expect(classify(2)).toBe("two-or-three"));
  test("classify default", () => expect(classify(99)).toBe("other"));
  // Fallthrough com strings
  test("fallthrough a -> b", () => expect(steps("a")).toBe("ab"));
  test("fallthrough b stop", () => expect(steps("b")).toBe("b"));
  test("fallthrough c", () => expect(steps("c")).toBe("c"));
});
