// Cross-runtime: destructuring rest drains the remaining custom iterator values.
const iterable = {
  *[Symbol.iterator]() {
    yield "a";
    yield "b";
    yield "c";
    yield "d";
  },
};
const [head, ...tail] = iterable;
console.log(head, tail.join(","));

