import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

function main() {
  // `??`: se null/0, usa rhs.
  const a: i32 = 0;
  const b: i32 = 42;
  print(`${a ?? 99}`);   // 99 (0 tratado como null no RTS)
  print(`${b ?? 99}`);   // 42
  print(`${7 ?? 99}`);   // 7

  // ??= usa mesma rota; usa compound assign follow-up.
}

function twice(x: i32): i32 { return x * 2; }

function call_opt(fn: i64, x: i32): i64 {
  // optional call: se fn e null (0), retorna sentinel undefined; senao invoca.
  return fn?.(x);
}

function main2() {
  const r1 = call_opt(twice, 10);
  print(`${r1}`);  // 20 (sentinel nao aplica em result truthy)
  const r2 = call_opt(0, 10);
  // r2 eh i64::MIN+2 (sentinel undefined). Aqui validamos via comparacao.
  print(`${(r2 as any) === undefined ? "undefined" : "X"}`);
}

main();
main2();

describe("fixture:nullish_optional", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("99\n42\n7\n20\nundefined\n");
  });
});
