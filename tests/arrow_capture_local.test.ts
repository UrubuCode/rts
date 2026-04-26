import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __out: string = "";
function p(s: string): void { __out += s + "\n"; }

// 1. Captura mutável de local (fase 3 clássico).
function counter(): void {
  let count: i64 = 0;
  const inc = () => { count = count + 1; };
  inc();
  inc();
  inc();
  p(`count=${count}`);
}
counter();

// 2. Captura de parâmetro.
function adder(start: i64): void {
  let total: i64 = 0;
  const add = () => { total = total + start; };
  add();
  add();
  p(`total=${total}`);
}
adder(7);

// 3. Múltiplas arrows compartilhando a mesma local.
function multi(): void {
  let n: i64 = 10;
  const dec = () => { n = n - 1; };
  const dbl = () => { n = n * 2; };
  dec();   // 9
  dbl();   // 18
  dec();   // 17
  p(`n=${n}`);
}
multi();

// 4. Arrow declarada dentro de if — só lift quando o ramo é tomado.
function branchy(flag: i32): void {
  let x: i64 = 0;
  if (flag > 0) {
    const f = () => { x = x + 100; };
    f();
    f();
  }
  p(`x=${x}`);
}
branchy(1);
branchy(0);

// 5. Sem captura, só verifica que continua funcionando.
function plain(): void {
  const sayHi = () => { p("hi"); };
  sayHi();
  sayHi();
}
plain();

describe("fixture:arrow_capture_local", () => {
  test("matches expected stdout", () => {
    expect(__out).toBe("count=3\ntotal=14\nn=17\nx=200\nx=0\nhi\nhi\n");
  });
});
