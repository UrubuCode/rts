// Cross-runtime: computed class member names and symbols.
const key = "sum";
const iterKey = Symbol.iterator;
const reg = Symbol.for("demo");
class C {
  [key](a: number, b: number) { return a + b; }
  [iterKey]() { return [1, 2][Symbol.iterator](); }
  [reg]() { return "ok"; }
}
const c = new C();
console.log("sum=" + (c as any).sum(2, 3));
console.log("iter=" + Array.from(c as any).join(","));
console.log("sym=" + (c as any)[reg]());
