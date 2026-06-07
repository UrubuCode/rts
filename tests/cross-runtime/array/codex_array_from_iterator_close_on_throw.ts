// Cross-runtime: Array.from closes iterator when mapper throws.
const log: string[] = [];
const iterable = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        log.push("next" + i);
        return { value: i++, done: i > 5 };
      },
      return() {
        log.push("return");
        return { done: true };
      }
    };
  }
};

try {
  Array.from(iterable, (x: number) => {
    if (x === 2) throw new Error("stop");
    return x;
  });
} catch (e: any) {
  console.log(e.message);
}
console.log(log.join(","));
