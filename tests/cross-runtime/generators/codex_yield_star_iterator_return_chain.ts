// Cross-runtime: yield* forwards return through inner iterator.
const log: string[] = [];
const inner = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        log.push("inner-next" + i);
        return { value: i++, done: i > 3 };
      },
      return(v?: any) {
        log.push("inner-return:" + v);
        return { value: "closed", done: true };
      }
    };
  }
};

function* outer() {
  const v = yield* inner;
  log.push("outer-after:" + v);
}

const it = outer();
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.return("bye")));
console.log(log.join("|"));
