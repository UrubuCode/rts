import { io } from "rts";

export function testArithmeticExpressions(): void {
  const simple = 1 + 1;
  const grouped = (1 + 1);
  const precedence = 2 + 3 * 4;
  const nested = (2 + 3) * (4 + 1);
  const ratio = (10 - 4) / 3;
  const modulo = 10 % 3;

  io.print("[tests/arithmetic] simple=" + simple);
  io.print("[tests/arithmetic] grouped=" + grouped);
  io.print("[tests/arithmetic] precedence=" + precedence);
  io.print("[tests/arithmetic] nested=" + nested);
  io.print("[tests/arithmetic] ratio=" + ratio);
  io.print("[tests/arithmetic] modulo=" + modulo);
  io.print("[tests/arithmetic] concat-simple=" + (1 + 1) + " hello from package console");
  io.print("[tests/arithmetic] concat-grouped=" + ((1 + 1)) + " hello from package console");
}
