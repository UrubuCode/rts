// Cross-runtime: `await` resumptions, `.then` reactions and `queueMicrotask`
// callbacks all share ONE FIFO queue. Two independent async chains started in a
// known order therefore interleave step by step; the merge order is the claim.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const steps: string[] = [];

async function chain(tag: string, depth: number) {
  for (let i = 1; i <= depth; i++) {
    await null;
    steps.push(tag + i);
  }
}

// 1) two chains of equal depth alternate, starting with the one begun first
chain("A", 4);
chain("B", 4);

// 2) a queueMicrotask ladder started between them takes its place in the queue
function ladder(tag: string, left: number) {
  queueMicrotask(function () {
    steps.push(tag + (5 - left));
    if (left > 1) ladder(tag, left - 1);
  });
}
ladder("Q", 4);

// 3) and a .then ladder does too
let chainP: Promise<void> = Promise.resolve();
for (let i = 1; i <= 4; i++) {
  const k = i;
  chainP = chainP.then(function () { steps.push("T" + k); });
}

steps.push("sync");

(async function () {
  for (let i = 0; i < 16; i++) await null;
  log("interleave=" + steps.join(","));
  log("syncWasFirst=" + (steps[0] === "sync"));
  log("count=" + steps.length);

  // 4) queueMicrotask and .then enqueued alternately keep their exact order
  steps.length = 0;
  for (let i = 1; i <= 5; i++) {
    const k = i;
    queueMicrotask(function () { steps.push("q" + k); });
    Promise.resolve().then(function () { steps.push("p" + k); });
  }
  for (let i = 0; i < 4; i++) await null;
  log("strictFifo=" + steps.join(","));

  // 5) a microtask that enqueues two more: the two land at the tail, after
  //    everything already queued
  steps.length = 0;
  queueMicrotask(function () {
    steps.push("m1");
    queueMicrotask(function () { steps.push("m1a"); });
    queueMicrotask(function () { steps.push("m1b"); });
  });
  queueMicrotask(function () { steps.push("m2"); });
  queueMicrotask(function () { steps.push("m3"); });
  for (let i = 0; i < 4; i++) await null;
  log("tailInsertion=" + steps.join(","));

  // 6) an async function resumes exactly where a queueMicrotask enqueued at the
  //    same moment would have
  steps.length = 0;
  (async function () { await null; steps.push("awaited"); })();
  queueMicrotask(function () { steps.push("qm-after"); });
  for (let i = 0; i < 3; i++) await null;
  log("awaitIsAMicrotask=" + steps.join(","));

  // 7) queueMicrotask answers undefined and refuses a non-callable argument
  log("returnValue=" + String(queueMicrotask(function () { })));
  log("nonCallable=" + (function () {
    try { (queueMicrotask as any)(42); return "no"; } catch (e: any) { return e.constructor.name; }
  })());
  log("noArgument=" + (function () {
    try { (queueMicrotask as any)(); return "no"; } catch (e: any) { return e.constructor.name; }
  })());
  // (queueMicrotask.length is NOT asserted: Bun reports 2 and Node 1.)
  log("type=" + typeof queueMicrotask);

  // 8) the callback is invoked with no arguments
  let argc = -1;
  queueMicrotask(function () { argc = arguments.length; });
  await null;
  log("callbackArgs=" + argc);

  // 9) a deep chain of awaits inside one function costs one tick per await, and
  //    a competing ladder proves it
  steps.length = 0;
  (async function () {
    for (let i = 1; i <= 3; i++) { await null; steps.push("deep" + i); }
  })();
  let rung: Promise<void> = Promise.resolve();
  for (let i = 1; i <= 3; i++) {
    const k = i;
    rung = rung.then(function () { steps.push("rung" + k); });
  }
  for (let i = 0; i < 8; i++) await null;
  log("oneTickPerAwait=" + steps.join(","));

  // 10) `await` inside a `try`/`finally` costs the same ticks as a bare one
  steps.length = 0;
  (async function () {
    try { await null; steps.push("inTry"); } finally { steps.push("inFinally"); }
    await null;
    steps.push("afterFinally");
  })();
  (async function () { await null; steps.push("bare1"); await null; steps.push("bare2"); })();
  for (let i = 0; i < 6; i++) await null;
  log("tryFinallyTicks=" + steps.join(","));

  // 11) a rejected await resumes on the catch path at the same tick a fulfilled
  //     one would have resumed on
  steps.length = 0;
  (async function () {
    try { await Promise.reject("R"); } catch (e: any) { steps.push("caught:" + e); }
  })();
  (async function () { await Promise.resolve("F"); steps.push("fulfilled"); })();
  for (let i = 0; i < 4; i++) await null;
  log("rejectionSameTick=" + steps.join(","));

  // 12) a microtask enqueued from a `finally` reaction joins the tail like any
  //     other
  steps.length = 0;
  Promise.resolve().finally(function () { steps.push("finallyCb"); queueMicrotask(function () { steps.push("fromFinally"); }); });
  queueMicrotask(function () { steps.push("sibling"); });
  for (let i = 0; i < 5; i++) await null;
  log("finallyReaction=" + steps.join(","));

  console.log("end");
})().catch(function () { console.log("UNEXPECTED"); });
