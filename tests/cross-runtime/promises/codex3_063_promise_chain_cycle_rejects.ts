// Cross-runtime: resolving a promise chain with itself rejects with TypeError.
let chained: Promise<any>;
chained = Promise.resolve("start").then(() => chained);
chained.then(
  () => console.log("fulfilled"),
  (error) => console.log(error instanceof TypeError, error.name),
);

