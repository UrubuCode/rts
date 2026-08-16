// Cross-runtime: queueMicrotask and Promise.then share ONE FIFO queue, and a
// microtask queued from inside a microtask joins the tail of the SAME drain.
// Focus: the interleaving of the two APIs at every nesting depth, with no timer
// anywhere in the file.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

log("sync start");

// 1) alternating registrations drain strictly in registration order
queueMicrotask(function () { log("qm A"); });
Promise.resolve().then(function () { log("then A"); });
queueMicrotask(function () { log("qm B"); });
Promise.resolve().then(function () { log("then B"); });

// 2) a microtask that queues two more: both land AFTER everything already
//    queued, in the order they were queued
queueMicrotask(function () {
  log("qm C (queues C1, C2)");
  queueMicrotask(function () { log("qm C1"); });
  Promise.resolve().then(function () { log("then C2"); });
});

// 3) a then that queues a microtask, and a microtask that queues a then
Promise.resolve().then(function () {
  log("then D (queues D1)");
  queueMicrotask(function () { log("qm D1"); });
});

// 4) a nested chain of `then`s: each link costs one more turn
Promise.resolve()
  .then(function () { log("chain 1"); })
  .then(function () { log("chain 2"); })
  .then(function () { log("chain 3"); });

// 5) an async function's continuation is just another microtask
(async function () {
  log("async body before await");
  await undefined;
  log("async after first await");
  await undefined;
  log("async after second await");
})();

// 6) recursion through queueMicrotask stays inside one drain
let depth = 0;
function recur() {
  depth++;
  if (depth <= 3) {
    log("recur depth " + depth);
    queueMicrotask(recur);
  } else {
    log("recur done at " + depth);
  }
}
queueMicrotask(recur);

// 7) queueMicrotask returns undefined and takes a callback only
log("qmReturns=" + String(queueMicrotask(function () { log("qm E"); })));
log("qmNonCallable=" + (function () {
  try { (queueMicrotask as any)(42); return "no"; } catch (e: any) { return e.constructor.name; }
})());

// 8) a throw inside queueMicrotask is CAUGHT here so it cannot end the process
queueMicrotask(function () {
  try {
    (function () { throw new RangeError("contained"); })();
  } catch (e: any) {
    log("caught inside microtask: " + e.constructor.name);
  }
});

log("sync end");

// 9) the last word, from the deepest continuation this file has
let tail: Promise<void> = Promise.resolve();
for (let i = 0; i < 12; i++) tail = tail.then(function () { return undefined; });
tail.then(function () {
  queueMicrotask(function () {
    queueMicrotask(function () { console.log("end"); });
  });
});
