// Cross-runtime: instance fields initialize in source order before constructor body.
const seen: string[] = [];
class Example {
  a = (seen.push("a"), 1);
  b = (seen.push("b"), this.a + 1);
  constructor() { seen.push("ctor"); }
}
const e = new Example();
console.log(e.a, e.b);
console.log(seen.join(","));

