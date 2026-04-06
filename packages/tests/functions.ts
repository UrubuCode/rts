import { io } from "rts";

function multiplyAndOffset(a: number, b: number, offset: number): number {
  const product = a * b;
  return product + offset;
}

function formatResult(label: string, value: number): string {
  return label + "=" + value;
}

function compareAsText(a: number, b: number): string {
  const greater = a > b;
  const same = a === b;
  return "greater:" + greater + " same:" + same;
}

export function testComplexFunctions(): void {
  const first = multiplyAndOffset(2, 3, 10);
  const second = multiplyAndOffset(4, 5, 1);
  const third = multiplyAndOffset(3, 3, 7);
  const composed = formatResult("first", first) + " | " + formatResult("second", second);

  io.print("[tests/functions] " + composed);
  io.print("[tests/functions] third=" + third);
  io.print("[tests/functions] compare-first-second " + compareAsText(first, second));
  io.print("[tests/functions] logic=" + ((first > second) || (first === 16)));
}
