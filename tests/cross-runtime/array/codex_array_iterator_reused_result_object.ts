// Cross-runtime: custom iterator reuses result object while Array.from snapshots values.
const result: any = { value: 0, done: false };
const iterable = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        result.value = { n: i };
        result.done = i++ >= 3;
        return result;
      }
    };
  }
};

const out = Array.from(iterable, (x: any) => x.n);
console.log(out.join(","));
