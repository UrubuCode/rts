// Cross-runtime compatibility: AggregateError and Error.cause.
const err = new Error("root", { cause: "origin" as any });
console.log("cause=" + String((err as any).cause));

const agg = new AggregateError([new Error("a"), "b"], "many");
console.log("name=" + agg.name);
console.log("msg=" + agg.message);
console.log("count=" + Array.from(agg.errors).length);
console.log(
  "items=" +
    Array.from(agg.errors)
      .map((item: any) => (item instanceof Error ? item.message : String(item)))
      .join(","),
);
