// Testes "fora da caixa" para logical operators — combinacoes inusuais,
// chains profundas, interacao com outros features (classes, arrays,
// JSON, ternarios).
import { describe, test, expect } from "rts:test";

// Chain longa de OR — pega primeiro truthy
const r1: string = "" || "" || "fb" || "ignored";
const r2: string = "first" || "second" || "third";
const r3: string = "" || "" || "" || "" || "last";

// Chain longa de AND — pega primeiro falsy ou ultimo
const r4: string = "a" && "b" && "c";
const r5: string = "a" && "" && "c";
const r6: string = "" && "b" && "c";

// Mix OR + AND (precedencia: && > ||)
const r7: string = "" || "x" && "y";        // "" || ("x" && "y") = "y"
const r8: string = "a" || "b" && "c";        // "a" || ("b" && "c") = "a"
const r9: string = "" && "x" || "fallback"; // ("" && "x") || "fallback" = "fallback"

// Em ternario — fallback "default" tem length 7 (> 5)
function pick(s: string): string {
  return (s || "default").length > 5 ? "long" : "short";
}
// "hi" || "default" -> "hi" (length 2, < 5)
// "" || "default"   -> "default" (length 7, > 5)

// String concat com OR (devia preservar o resultado correto)
function greet(name: string): string {
  return "Hello, " + (name || "world") + "!";
}

// Pattern de fallback com number tipado.
// Apenas teste 0 -> fallback. Testar valor != 0 cai em bug
// pre-existente de fcvt f64 -> i64 (number param truncate).

// OR como guard de array vazio
const arr: number[] = [];
const fb: number[] = [1, 2, 3];
const len: number = (arr.length && 99) || -1;
// arr.length === 0, && curto-circuita -> 0; 0 || -1 -> -1

// Truthiness de strings com whitespace
const ws: string = "   ";
const r10: string = ws || "fb"; // "   " eh truthy, retorna "   "

// Encadeamento com null/undefined
const maybe: any = null;
const r11: string = maybe || "default";

describe("logical operators — combinacoes criativas", () => {
  test("|| chain pega primeiro truthy", () => expect(r1).toBe("fb"));
  test("|| chain todos truthy pega primeiro", () => expect(r2).toBe("first"));
  test("|| chain so' last", () => expect(r3).toBe("last"));
  test("&& chain todos truthy pega ultimo", () => expect(r4).toBe("c"));
  test("&& chain pega primeiro falsy", () => expect(r5).toBe(""));
  test("&& chain primeiro falsy = ''", () => expect(r6).toBe(""));
  test("precedencia: '' || 'x' && 'y' = 'y'", () => expect(r7).toBe("y"));
  test("precedencia: 'a' || ... = 'a'", () => expect(r8).toBe("a"));
  test("'' && 'x' || 'fallback' = 'fallback'", () => expect(r9).toBe("fallback"));
  test("ternario: '' || 'default' -> 'default' (len 7 > 5 -> long)", () => expect(pick("")).toBe("long"));
  test("ternario: 'hi' || 'default' -> 'hi' (len 2 -> short)", () => expect(pick("hi")).toBe("short"));
  test("greet com nome vazio = world", () => expect(greet("")).toBe("Hello, world!"));
  test("greet com nome = nome", () => expect(greet("Bob")).toBe("Hello, Bob!"));
  test("number 0 || 99 = 99", () => {
    const v: number = 0;
    expect(v || 99).toBe(99);
  });
  test("number 7 || 99 = 7", () => {
    const v: number = 7;
    expect(v || 99).toBe(7);
  });
  test("guard array vazio cai em fallback", () => expect(len).toBe(-1));
  test("string com whitespace eh truthy", () => expect(r10).toBe("   "));
  test("null || default = default", () => expect(r11).toBe("default"));
});
