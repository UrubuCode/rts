// Cross-runtime: IteratorClose looks up return once and calls it with the iterator receiver.
const seen: string[] = [];
const iterator: any = {
  next() { return { value: 1, done: false }; },
  get return() {
    seen.push("get-return");
    return function () { seen.push("call-return:" + (this === iterator)); return { done: true }; };
  },
};
const iterable = { [Symbol.iterator]() { return iterator; } };
for (const value of iterable) { console.log(value); break; }
console.log(seen.join(","));

