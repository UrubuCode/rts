import { describe, test, expect } from "rts:test";

// Regression (generators MVP): protocolo `.next()/.value/.done` para
// generators FINITOS, via cursor lateral por-handle (on-demand). Antes,
// `g().next()` crashava (SIGILL). Generators INFINITOS (`while(true) yield`)
// ainda exigem state-machine real (follow-up #477).

let out = "";
function print(v: string): void { out += v + "\n"; }

// .next()/.value/.done
function* g() { yield 1; yield 2; yield 3; }
const it = g();
print("" + it.next().value);   // 1
print("" + it.next().value);   // 2
print("" + it.next().value);   // 3
print("" + it.next().done);    // true

// for-of de generator finito continua funcionando (eager-buffer Vec)
function* letters() { yield 10; yield 20; yield 30; }
let sum = 0;
for (const v of letters()) sum = sum + v;
print("sum=" + sum);  // 60

// spread
const arr = [...letters()];
print("len=" + arr.length);  // 3

// duas instâncias do mesmo generator têm cursores independentes
function* nums() { yield 5; yield 6; }
const a = nums();
const b = nums();
print("ab=" + a.next().value + "," + b.next().value + "," + a.next().value);  // 5,5,6

describe("generator next protocol (finite)", () => {
  test("next/value/done", () => expect(out.startsWith("1\n2\n3\ntrue\n")).toBe(true));
  test("for-of preservado", () => expect(out.indexOf("sum=60\n") >= 0).toBe(true));
  test("spread preservado", () => expect(out.indexOf("len=3\n") >= 0).toBe(true));
  test("cursores independentes", () => expect(out.indexOf("ab=5,5,6\n") >= 0).toBe(true));
});
