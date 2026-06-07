// Cross-runtime: AggregateError consumes iterable in order.
function* errors() {
  yield new Error("a");
  yield "plain";
  yield 3;
}

const err = new AggregateError(errors(), "many");
console.log(err.name + ":" + err.message);
console.log(err.errors.length);
console.log(err.errors.map((e: any) => e && e.message ? e.message : String(e)).join(","));
