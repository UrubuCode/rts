import { describe, test, expect } from "rts:test";

// Regression (generators MVP): `return X` num generator finito. O primeiro
// `.next()` após esgotar os yields devolve `{value: X, done: true}`; os
// seguintes `{value: undefined, done: true}` (JS spec). for-of NÃO inclui o
// valor de return entre os iterados.

let out = "";
function print(v: string): void { out += v + "\n"; }

function* withReturn() { yield 1; yield 2; return 99; }
const g = withReturn();
print("" + g.next().value);   // 1
print("" + g.next().value);   // 2
print("" + g.next().value);   // 99 (valor de return)
print("" + g.next().done);    // true

// for-of não itera o valor de return (só os yields)
function* a() { yield 10; yield 20; return 999; }
let sum = 0;
for (const v of a()) sum += v;
print("sum=" + sum);  // 30 (não inclui 999)

describe("generator return value", () => {
  test("return X via next ao esgotar", () => expect(out.startsWith("1\n2\n99\ntrue\n")).toBe(true));
  test("for-of ignora valor de return", () => expect(out.indexOf("sum=30\n") >= 0).toBe(true));
});
