// Cross-runtime: Promise.allSettled (deterministic)
const p1 = Promise.resolve(1);
const p2 = Promise.reject("error");
const p3 = Promise.resolve(3);

Promise.allSettled([p1, p2, p3]).then(results => {
  console.log("count=" + results.length);
  console.log("status0=" + results[0].status);
  console.log("value0=" + (results[0] as any).value);
  console.log("status1=" + results[1].status);
  console.log("reason1=" + (results[1] as any).reason);
  console.log("status2=" + results[2].status);
  console.log("value2=" + (results[2] as any).value);
});

setTimeout(() => {}, 10);
