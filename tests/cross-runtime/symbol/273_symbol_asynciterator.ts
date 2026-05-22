// Cross-runtime: Symbol.asyncIterator with for-await-of.
const asyncIter = {
  async *[Symbol.asyncIterator]() {
    yield 1; yield 2; yield 3;
  },
};
const out: number[] = [];
for await (const n of asyncIter as any) out.push(n * 2);
console.log("iter=" + out.join(","));
