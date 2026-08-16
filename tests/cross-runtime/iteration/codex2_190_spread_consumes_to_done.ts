// Cross-runtime: spread repeatedly calls next until the first done result.
const seen: string[] = [];
const iterable = {
  [Symbol.iterator]() {
    let n = 0;
    return {
      next() {
        seen.push("next:" + n);
        return n < 3 ? { value: n++, done: false } : { value: 99, done: true };
      },
    };
  },
};
console.log([...iterable].join(","));
console.log(seen.join("|"));

