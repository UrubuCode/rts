// Comparacao de igualdade entre membros de string enum.
import { io, gc } from "rts";

enum Direction {
  Up = "u",
  Down = "d",
  Left = "l",
  Right = "r",
}

function describe(d: string): string {
  if (d == Direction.Up) return "subindo";
  if (d == Direction.Down) return "descendo";
  if (d == Direction.Left) return "esquerda";
  if (d == Direction.Right) return "direita";
  return "desconhecido";
}

io.print(describe(Direction.Up));
io.print(describe(Direction.Right));
io.print(describe("x"));
