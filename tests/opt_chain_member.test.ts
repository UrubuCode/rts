import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#271) \`obj?.prop\` agora null-guard: retorna 0 (representando undefined)
// quando obj e' null/0, em vez de chamar member_expr direto.

// 1. Null obj — retorna 0
const nullObj: { a: number } | null = null;
print(`${nullObj?.a}`);    // 0 (representa undefined)

// 2. Obj normal — acessa
const obj = { a: 42, b: 99 };
print(`${obj?.a}`);        // 42
print(`${obj?.b}`);        // 99

// 3. Em fn user com possivel null
function getValue(o: { x: number } | null): number {
  const v = o?.x;
  if (v === 0) return -1;
  return v;
}
print(`${getValue({ x: 7 })}`);   // 7
print(`${getValue(null)}`);       // -1 (v=0 vira -1)

// 4. Encadeado com ternario
const cond: boolean = true;
const o = cond ? { val: 100 } : null;
print(`${o?.val}`);  // 100

const cond2: boolean = false;
const o2 = cond2 ? { val: 200 } : null;
print(`${o2?.val}`);  // 0

// 5. Em classe
class Container {
  data: { count: number } | null = null;
}
const c = new Container();
print(`${c.data?.count}`);    // 0 (data e' null)
c.data = { count: 5 };
print(`${c.data?.count}`);    // 5

describe("opt_chain_member", () => {
  test("null guard funcional", () =>
    expect(__rtsCapturedOutput).toBe(
      "0\n" +              // 1
      "42\n99\n" +         // 2
      "7\n-1\n" +          // 3
      "100\n0\n" +         // 4
      "0\n5\n"             // 5
    ));
});
