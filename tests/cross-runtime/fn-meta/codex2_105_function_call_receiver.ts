// Cross-runtime: call supplies an explicit receiver and positional arguments.
function describe(this: any, a: number, b: number) {
  return this.name + ":" + (a + b);
}
console.log(describe.call({ name: "ctx" }, 2, 5));
console.log(describe.call({ name: "other" }, -1, 4));

