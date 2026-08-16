// Cross-runtime: Promise.any AggregateError errors retain input order, not rejection time.
const late = new Promise((_, reject) => Promise.resolve().then(() => reject("late")));
const early = Promise.reject("early");
Promise.any([late, early]).catch((error) => {
  console.log(error instanceof AggregateError, error.name);
  console.log(error.errors.join(","));
});

