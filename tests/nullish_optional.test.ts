import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

function main() {
  // `??`: so' null/undefined usam o rhs (JS fiel — 0 e' um valor valido).
  const a: i32 = 0;
  const b: i32 = 42;
  print(`${a ?? 99}`);   // 0 (JS: 0 nao e' nullish)
  print(`${b ?? 99}`);   // 42
  print(`${7 ?? 99}`);   // 7
  const n: any = null;
  print(`${n ?? 99}`);   // 99 (null e' nullish)
}

function twice(x: number): number { return x * 2; }

function call_opt(fn: any, x: number): any {
  // optional call: fn null/undefined → undefined; senao invoca.
  return fn?.(x);
}

function main2() {
  const r1 = call_opt(twice, 10);
  print(`${r1}`);  // 20
  const r2 = call_opt(null, 10);
  print(`${r2 === undefined ? "undefined" : "X"}`);
}

main();
main2();

describe("fixture:nullish_optional", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n42\n7\n99\n20\nundefined\n");
  });
});
