// Cross-runtime: for-await break awaits an asynchronous iterator return method.
const seen: string[] = [];
const iterable = {
  [Symbol.asyncIterator]() {
    let n = 0;
    return {
      async next() { seen.push("next"); return { value: ++n, done: false }; },
      async return() { await Promise.resolve(); seen.push("return"); return { done: true }; },
    };
  },
};
async function main() {
  for await (const value of iterable) {
    seen.push("value:" + value);
    break;
  }
  seen.push("after");
  console.log(seen.join("|"));
}
main();

