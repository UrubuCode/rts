// Cross-runtime: AggregateError construction and errors iterable.
const agg = new AggregateError([new Error("a"), "b"], "many");
console.log("name=" + agg.name);
console.log("msg=" + agg.message);
console.log("len=" + Array.from(agg.errors).length);
console.log("items=" + Array.from(agg.errors).map((x: any) => x instanceof Error ? x.message : String(x)).join("|"));
