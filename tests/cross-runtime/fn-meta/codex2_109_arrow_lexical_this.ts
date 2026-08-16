// Cross-runtime: arrow functions retain lexical this through extraction.
const o = {
  value: 6,
  make() {
    return (n: number) => this.value * n;
  },
};
const f = o.make();
console.log(f(3));
console.log(f.call({ value: 100 }, 2));

