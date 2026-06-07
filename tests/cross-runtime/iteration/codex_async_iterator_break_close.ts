// Cross-runtime: async iterator return() is awaited when breaking.
const log: string[] = [];
const asyncIterable = {
  [Symbol.asyncIterator]() {
    let i = 0;
    return {
      async next() {
        log.push("next" + i);
        return { value: i++, done: i > 4 };
      },
      async return() {
        log.push("return-start");
        await Promise.resolve();
        log.push("return-end");
        return { done: true };
      }
    };
  }
};

(async () => {
  let sum = 0;
  for await (const x of asyncIterable) {
    sum += x;
    if (x === 1) break;
  }
  console.log(sum);
  console.log(log.join(","));
})();
