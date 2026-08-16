// Cross-runtime: a returned method combines receiver state with captured state.
function make(offset: number) {
  return {
    value: 3,
    calc(this: any, n: number) { return this.value + offset + n; },
  };
}
const a = make(10);
const extracted = a.calc;
console.log(a.calc(2));
console.log(extracted.call({ value: 20 }, 2));

