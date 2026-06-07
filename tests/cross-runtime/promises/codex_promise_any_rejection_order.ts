// Cross-runtime: Promise.any AggregateError preserves rejection order.
Promise.any([
  Promise.reject("a"),
  Promise.reject(new Error("b")),
  { then(_resolve: any, reject: any) { reject("c"); } }
]).catch((e: any) => {
  console.log(e.constructor.name);
  console.log(e.errors.map((x: any) => x && x.message ? x.message : String(x)).join(","));
});
