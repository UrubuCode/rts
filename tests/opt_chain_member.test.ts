import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#271 + #456) `obj?.prop` em obj null produz "undefined" quando coerced
// em template literal (alinhado a JS); o valor SSA subjacente eh sentinel
// i64::MIN+2 (undefined) para que comparacoes `=== undefined` funcionem.

// 1. Null obj — retorna "undefined" em template literal
const nullObj: { a: number } | null = null;
print(`${nullObj?.a}`);    // "undefined"

// 2. Obj normal — acessa
const obj = { a: 42, b: 99 };
print(`${obj?.a}`);        // 42
print(`${obj?.b}`);        // 99

// 3. Em fn user com possivel null
function getValue(o: { x: number } | null): number {
  const v = o?.x;
  if ((v as any) === undefined) return -1;
  return v;
}
print(`${getValue({ x: 7 })}`);   // 7
print(`${getValue(null)}`);       // -1 (v=undefined vira -1)

// 4. Encadeado com ternario
const cond: boolean = true;
const o = cond ? { val: 100 } : null;
print(`${o?.val}`);  // 100

const cond2: boolean = false;
const o2 = cond2 ? { val: 200 } : null;
print(`${o2?.val}`);  // "undefined"

// 5. Em classe
class Container {
  data: { count: number } | null = null;
}
const c = new Container();
print(`${c.data?.count}`);    // "undefined" (data e' null)
c.data = { count: 5 };
print(`${c.data?.count}`);    // 5

describe("opt_chain_member", () => {
  test("null guard funcional", () =>
    expect(__rtsCapturedOutput).toBe(
      "undefined\n" +      // 1
      "42\n99\n" +         // 2
      "7\n-1\n" +          // 3
      "100\nundefined\n" + // 4
      "undefined\n5\n"     // 5
    ));
});
