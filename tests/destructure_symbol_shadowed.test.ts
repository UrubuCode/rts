import { describe, test, expect } from "rts:test";

// Destructuring an array reaches for the well-known iterator symbol. It must
// reach the SYMBOL, not the global binding spelled `Symbol`: a local of that
// name is an ordinary name a program may declare, and it must not decide
// whether a destructuring works. `for`-`of` already read the key directly.

function destructureUnderShadowedSymbol(source: number[]): number {
  const Symbol = null;
  const [x, y] = source;
  return (x as number) + (y as number);
}

function forOfUnderShadowedSymbol(source: number[]): number {
  const Symbol = null;
  let total = 0;
  for (const value of source) total += value;
  return total;
}

function parameterPatternUnderShadowedSymbol([x, y]: number[]): number {
  return x + y;
}

describe("fixture:destructure_symbol_shadowed", () => {
  test("a local named Symbol does not break array destructuring", () => {
    expect(destructureUnderShadowedSymbol([1, 2])).toBe(3);
  });

  test("for-of was already right and stays right", () => {
    expect(forOfUnderShadowedSymbol([1, 2])).toBe(3);
  });

  test("a parameter pattern destructures the same way", () => {
    const Symbol = null;
    expect(parameterPatternUnderShadowedSymbol([4, 5])).toBe(9);
  });
});
