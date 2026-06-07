// Cross-runtime: IteratorClose when for..of exits early.
const log: string[] = [];
const iterable = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        log.push("next" + i);
        return { value: i++, done: i > 4 };
      },
      return() {
        log.push("return");
        return { done: true };
      }
    };
  }
};

let sum = 0;
for (const x of iterable) {
  sum += x;
  if (x === 2) break;
}
console.log(sum);
console.log(log.join(","));
