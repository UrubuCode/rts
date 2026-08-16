// Cross-runtime: yield-star closes a delegate lacking throw before raising TypeError.
const seen: string[] = [];
const delegate = {
  [Symbol.iterator]() {
    return {
      next() { seen.push("next"); return { value: 1, done: false }; },
      return() { seen.push("return"); return { done: true }; },
    };
  },
};
function* outer() { yield* delegate; }
const it = outer();
console.log(JSON.stringify(it.next()));
let threw = false;
try { it.throw(new Error("x")); } catch (e) { threw = e instanceof TypeError; }
console.log(threw, seen.join(","));

