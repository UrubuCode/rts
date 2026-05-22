// Cross-runtime: Promise microtask ordering vs macrotask (setTimeout).

const order: string[] = [];

// microtasks run before next macrotask
setTimeout(() => order.push("timeout"), 0);
Promise.resolve().then(() => order.push("micro1"));
Promise.resolve().then(() => order.push("micro2"));
queueMicrotask(() => order.push("queueMicrotask"));
Promise.resolve().then(() => order.push("micro3"));

setTimeout(() => {
  console.log(order.join(","));
  // micro1,micro2,queueMicrotask,micro3,timeout
}, 10);

// Chained .then order (interleaving between two chains)
const chainOrder: string[] = [];
Promise.resolve()
  .then(() => { chainOrder.push("1"); return 1; })
  .then(() => { chainOrder.push("2"); return 2; })
  .then(() => { chainOrder.push("3"); });

Promise.resolve()
  .then(() => chainOrder.push("A"))
  .then(() => chainOrder.push("B"))
  .then(() => chainOrder.push("C"));

setTimeout(() => {
  console.log(chainOrder.join(","));  // 1,A,2,B,3,C
}, 10);

// async/await: each await yields to microtask queue
// concurrent tasks interleave at each await point
const interleave: string[] = [];
async function taskA() {
  interleave.push("A1");
  await Promise.resolve();
  interleave.push("A2");
  await Promise.resolve();
  interleave.push("A3");
}
async function taskB() {
  interleave.push("B1");
  await Promise.resolve();
  interleave.push("B2");
  await Promise.resolve();
  interleave.push("B3");
}
async function runInterleaved() {
  const a = taskA();  // starts immediately, suspends at first await
  const b = taskB();  // starts immediately, suspends at first await
  await Promise.all([a, b]);
  console.log(interleave.join(","));  // A1,B1,A2,B2,A3,B3
}
runInterleaved();

// Promise rejection handled asynchronously
const rejected = Promise.reject(new Error("handled"));
rejected.catch(e => console.log("caught:" + e.message));
