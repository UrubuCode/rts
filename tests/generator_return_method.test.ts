import { describe, test, expect } from "rts:test";

// Regression (generators MVP): `gen.return(v)` encerra o generator
// antecipadamente — devolve `{value:v, done:true}` e marca esgotado, então
// `.next()` subsequente dá done:true. Antes crashava (SIGILL, sem handler).

let out = "";
function print(v: string): void { out += v + "\n"; }

function* g() { yield 1; yield 2; yield 3; }
const it = g();
print("" + it.next().value);    // 1
print("" + it.return(99).value); // 99 (encerra)
print("" + it.next().done);     // true (esgotado, não continua yield 2)

describe("generator return method", () => {
  test("return(v) encerra e devolve v", () =>
    expect(out).toBe("1\n99\ntrue\n"));
});
