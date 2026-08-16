// Cross-runtime: iterator result done is tested by truthiness.
let step = 0;
const iterable = {
  [Symbol.iterator]() {
    return {
      next() {
        step++;
        if (step === 1) return { value: "a", done: 0 as any };
        if (step === 2) return { value: "b", done: "" as any };
        return { value: "hidden", done: "yes" as any };
      },
    };
  },
};
console.log([...iterable].join(","));
console.log(step);

