// Cross-runtime: each iteration requests a fresh iterator from the iterable.
let created = 0;
const iterable = {
  [Symbol.iterator]() {
    created++;
    let n = 0;
    return { next: () => n < 3 ? { value: ++n, done: false } : { value: undefined, done: true } };
  },
};
console.log([...iterable].join(","));
console.log([...iterable].join(","));
console.log(created);

