// Cross-runtime: return on a delegating generator forwards into the delegate.
const seen: string[] = [];
const delegate = {
  [Symbol.iterator]() {
    return {
      next() { seen.push("next"); return { value: 1, done: false }; },
      return(v: any) { seen.push("return:" + v); return { value: "closed:" + v, done: true }; },
    };
  },
};
function* outer() {
  const value = yield* delegate;
  return "outer:" + value;
}
const it = outer();
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.return("X")));
console.log(seen.join(","));

