// ONE thing: the evaluation ORDER of a compound assignment on a number.
// `o[k()] += rhs()` evaluates the reference ONCE, reads through the getter,
// evaluates the right side after the read, and writes back through the setter —
// so the observable trace is k, get, rhs, set, in that order.

const trace: string[] = [];

function makeCell(initial: number): any {
  let inner = initial;
  const cell: any = {};
  Object.defineProperty(cell, "v", {
    get() {
      trace.push("get:" + String(inner));
      return inner;
    },
    set(x: number) {
      trace.push("set:" + String(x));
      inner = x;
    },
    configurable: true,
  });
  return cell;
}

function rhs(name: string, value: number): number {
  trace.push("rhs:" + name);
  return value;
}

function report(label: string): void {
  console.log(label + " => " + trace.join(" "));
  trace.length = 0;
}

// --- the read happens before the right side is evaluated ---
let c = makeCell(10);
c.v += rhs("plus", 5);
report("plus_equals");
console.log("plus_result=" + c.v);
trace.length = 0;

c = makeCell(10);
c.v -= rhs("minus", 3);
report("minus_equals");

c = makeCell(10);
c.v *= rhs("times", 4);
report("times_equals");

c = makeCell(10);
c.v **= rhs("power", 2);
report("power_equals");

c = makeCell(10);
c.v /= rhs("div", 4);
report("div_equals");

c = makeCell(10);
c.v %= rhs("mod", 4);
report("mod_equals");

c = makeCell(-8);
c.v >>>= rhs("ushift", 1);
report("ushift_equals");

c = makeCell(-8);
c.v >>= rhs("shift", 1);
report("shift_equals");

c = makeCell(5);
c.v &= rhs("and", 3);
report("and_equals");

c = makeCell(5);
c.v |= rhs("or", 8);
report("or_equals");

c = makeCell(5);
c.v ^= rhs("xor", 1);
report("xor_equals");

c = makeCell(1);
c.v <<= rhs("lshift", 3);
report("lshift_equals");

// --- the results themselves ---
function value(initial: number, op: (o: any) => void): string {
  const cell = makeCell(initial);
  op(cell);
  const out = String(cell.v);
  trace.length = 0;
  return out;
}
console.log("results=" +
  value(10, (o) => { o.v += 5; }) + "," +
  value(10, (o) => { o.v -= 3; }) + "," +
  value(10, (o) => { o.v *= 4; }) + "," +
  value(10, (o) => { o.v /= 4; }) + "," +
  value(10, (o) => { o.v %= 4; }) + "," +
  value(2, (o) => { o.v **= 10; }) + "," +
  value(-8, (o) => { o.v >>>= 1; }) + "," +
  value(-8, (o) => { o.v >>= 1; }) + "," +
  value(5, (o) => { o.v &= 3; }) + "," +
  value(5, (o) => { o.v |= 8; }) + "," +
  value(5, (o) => { o.v ^= 1; }) + "," +
  value(1, (o) => { o.v <<= 3; }));

// --- the property KEY is evaluated exactly once, before the read ---
const keyLog: string[] = [];
function key(name: string): string {
  keyLog.push(name);
  return "n";
}
const bag: any = { n: 1 };
bag[key("k1")] += 10;
bag[key("k2")] *= 2;
console.log("key_evaluations=" + keyLog.join(",") + " value=" + bag.n);

// --- an index with a side effect moves exactly once ---
const arr: number[] = [10, 20, 30];
let i = 0;
arr[i++] += 5;
console.log("index_once=" + arr.join(",") + " i=" + i);
i = 0;
arr[i++] = arr[i++] + 100;
console.log("plain_assign_twice=" + arr.join(",") + " i=" + i);

// --- += on a string target concatenates: the operator is decided by the value ---
let s: any = "n";
s += 1;
s += 2;
console.log("string_plus=" + s + " typeof=" + typeof s);
let m: any = 1;
m += "1";
console.log("number_then_string=" + m + " typeof=" + typeof m);
let t: any = 1;
t += undefined;
console.log("plus_undefined=" + String(t));
let u: any = 1;
u += null;
console.log("plus_null=" + String(u));

// --- the object on the left is coerced through valueOf, once ---
const coerce: string[] = [];
let box: any = {
  valueOf() {
    coerce.push("valueOf");
    return 7;
  },
};
box += 1;
console.log("object_plus=" + String(box) + " typeof=" + typeof box + " calls=" + coerce.join(","));

// --- the logical forms skip the write entirely when they short-circuit ---
c = makeCell(5);
c.v ||= rhs("never", 99);
report("or_equals_truthy");
c = makeCell(0);
c.v ||= rhs("taken", 99);
report("or_equals_falsy");
c = makeCell(5);
c.v &&= rhs("taken2", 42);
report("and_equals_truthy");
c = makeCell(5);
c.v ??= rhs("never2", 1);
report("nullish_equals_present");
