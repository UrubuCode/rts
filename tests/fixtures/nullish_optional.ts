import { io } from "rts";

function main() {
  // `??`: se null/0, usa rhs.
  const a: i32 = 0;
  const b: i32 = 42;
  io.print(`${a ?? 99}`);   // 99 (0 tratado como null no RTS)
  io.print(`${b ?? 99}`);   // 42
  io.print(`${7 ?? 99}`);   // 7

  // ??= usa mesma rota; usa compound assign follow-up.
}

function twice(x: i32): i32 { return x * 2; }

function call_opt(fn: i64, x: i32): i32 {
  // optional call: se fn e null (0), retorna 0; senao invoca.
  return fn?.(x);
}

function main2() {
  io.print(`${call_opt(twice, 10)}`);  // 20
  io.print(`${call_opt(0, 10)}`);      // 0 (short-circuit)
}

main();
main2();
