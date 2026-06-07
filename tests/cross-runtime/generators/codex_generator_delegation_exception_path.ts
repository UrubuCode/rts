// Cross-runtime: yield* delegates throw when inner supports throw.
const log: string[] = [];
const inner = {
  [Symbol.iterator]() {
    return {
      next() { log.push("next"); return { value: "n", done: false }; },
      throw(e: any) { log.push("throw:" + e.message); return { value: "handled", done: false }; },
      return() { log.push("return"); return { done: true }; }
    };
  }
};

function* outer() {
  yield* inner;
}

const it = outer();
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.throw(new Error("E"))));
console.log(JSON.stringify(it.return("R")));
console.log(log.join("|"));
