// Cross-runtime: Promise.race and Promise.any (deterministic)
const p1 = Promise.resolve(1);
const p2 = Promise.resolve(2);

Promise.race([p1, p2]).then(val => {
  console.log("race=" + val);
});

Promise.any([p1, p2]).then(val => {
  console.log("any=" + val);
});

const p3 = Promise.reject("err");
const p4 = Promise.resolve(4);

Promise.race([p4, p3]).then(val => {
  console.log("race2=" + val);
});

Promise.any([p3, p4]).then(val => {
  console.log("any2=" + val);
});

setTimeout(() => {}, 10);
