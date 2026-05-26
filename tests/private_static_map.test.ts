import { describe, test, expect } from "rts:test";

// Regression (374, parcial): private static fields do tipo Map + static
// initializer blocks. Cobre os casos que agora funcionam (antes crashavam
// com SIGILL por causa do tipo Map perdido no static field).
//
// Follow-up: `new Map(arr.map(([k,v]) => [v,k]))` INLINE (Vec temporário de
// .map+parallel) ainda não popula — usar var intermediária funciona.

// (1) private static Map: set/get via método estático.
class Counter {
  static #counts: Map<string, number>;
  static { Counter.#counts = new Map(); }
  static bump(key: string): number {
    const cur = Counter.#counts.get(key) ?? 0;
    Counter.#counts.set(key, cur + 1);
    return Counter.#counts.get(key) ?? -1;
  }
}
const c1 = Counter.bump("a");
const c2 = Counter.bump("a");
const c3 = Counter.bump("b");

// (2) new Map(entries) com array de tuplas (chave numérica) via var.
const entries: [number, string][] = [[1, "one"], [2, "two"], [3, "three"]];
const table = new Map(entries);
const w1 = table.get(1);
const w3 = table.get(3);

// (3) new Map(arr.map(...)) com var intermediária (reverse lookup).
const rev = entries.map(([k, v]) => [v, k] as [string, number]);
const reverse = new Map(rev);
const n = reverse.get("two");

// (4) static initializer block simples (private static number).
class Config {
  static #version: string;
  static #limit: number;
  static { Config.#version = "1.0"; Config.#limit = 42; }
  static get version(): string { return Config.#version; }
  static get limit(): number { return Config.#limit; }
}

describe("private static map", () => {
  test("private static Map bump 1", () => expect(c1).toBe(1));
  test("private static Map bump 2 mesma chave", () => expect(c2).toBe(2));
  test("private static Map bump outra chave", () => expect(c3).toBe(1));
  test("new Map(tuplas) chave numérica get(1)", () => expect(w1).toBe("one"));
  test("new Map(tuplas) chave numérica get(3)", () => expect(w3).toBe("three"));
  test("new Map(arr.map) via var — reverse lookup", () => expect(n).toBe(2));
  test("static block: version", () => expect(Config.version).toBe("1.0"));
  test("static block: limit", () => expect(Config.limit).toBe(42));
});
