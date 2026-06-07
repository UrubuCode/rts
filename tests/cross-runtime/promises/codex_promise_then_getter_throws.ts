// Cross-runtime: Promise.resolve rejects if then getter throws.
const obj = Object.defineProperty({}, "then", {
  get() {
    throw new Error("nope");
  }
});

Promise.resolve(obj).then(
  () => console.log("fulfilled"),
  (e) => console.log(e.message)
);
