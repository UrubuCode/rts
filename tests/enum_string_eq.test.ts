import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Comparacao de igualdade entre membros de string enum.

enum Direction {
  Up = "u",
  Down = "d",
  Left = "l",
  Right = "r",
}

function fixture_describe(d: string): string {
  if (d == Direction.Up) return "subindo";
  if (d == Direction.Down) return "descendo";
  if (d == Direction.Left) return "esquerda";
  if (d == Direction.Right) return "direita";
  return "desconhecido";
}

print(fixture_describe(Direction.Up));
print(fixture_describe(Direction.Right));
print(fixture_describe("x"));

describe("fixture:enum_string_eq", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("subindo\ndireita\ndesconhecido\n");
  });
});
