// Cross-runtime: a listener runs SYNCHRONOUSLY, so a microtask it queues waits
// for the whole dispatch -- and for the rest of the surrounding script -- before
// running. Focus: where the microtask checkpoint falls around dispatchEvent.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const seen: string[] = [];

const t = new EventTarget();

// Three listeners, each queueing one microtask of each kind, in a fixed order.
t.addEventListener("go", function () {
  seen.push("L1");
  queueMicrotask(function () { seen.push("L1-qm"); });
  Promise.resolve().then(function () { seen.push("L1-then"); });
});
t.addEventListener("go", function () {
  seen.push("L2");
  Promise.resolve().then(function () { seen.push("L2-then"); });
  queueMicrotask(function () { seen.push("L2-qm"); });
});
t.addEventListener("go", function () {
  seen.push("L3");
});

seen.push("beforeDispatch");
t.dispatchEvent(new Event("go"));
seen.push("afterDispatch");

// A microtask queued after the dispatch joins the tail of the same queue.
queueMicrotask(function () { seen.push("tail-qm"); });

log("synchronousPart=" + seen.join(","));

(async function () {
  await null;
  log("afterOneTick=" + seen.join(","));

  // 1) two dispatches back to back: all six listeners run before any microtask
  seen.length = 0;
  t.dispatchEvent(new Event("go"));
  t.dispatchEvent(new Event("go"));
  log("twoDispatchesSync=" + seen.join(","));
  for (let i = 0; i < 3; i++) await null;
  log("twoDispatchesDrained=" + seen.join(","));

  // 2) a dispatch performed FROM a microtask: the listeners still run
  //    synchronously inside that microtask, ahead of the next one
  seen.length = 0;
  queueMicrotask(function () { seen.push("mt1-start"); t.dispatchEvent(new Event("go")); seen.push("mt1-end"); });
  queueMicrotask(function () { seen.push("mt2"); });
  log("beforeDrain=" + JSON.stringify(seen.join(",")));
  for (let i = 0; i < 4; i++) await null;
  log("dispatchInsideMicrotask=" + seen.join(","));

  // 3) an async listener: everything up to its first await is synchronous, the
  //    rest is a microtask, and dispatchEvent never waits for it
  seen.length = 0;
  const t3 = new EventTarget();
  t3.addEventListener("a", async function () {
    seen.push("async-start");
    await null;
    seen.push("async-after-await");
  });
  t3.addEventListener("a", function () { seen.push("sync-second"); });
  const ret = t3.dispatchEvent(new Event("a"));
  seen.push("dispatchReturned=" + ret);
  log("asyncListenerSync=" + seen.join(","));
  for (let i = 0; i < 3; i++) await null;
  log("asyncListenerDrained=" + seen.join(","));

  // 4) an async listener's returned promise is DISCARDED: a second dispatch
  //    does not wait for the first one's continuation
  seen.length = 0;
  t3.dispatchEvent(new Event("a"));
  t3.dispatchEvent(new Event("a"));
  log("twoAsyncSync=" + seen.join(","));
  for (let i = 0; i < 4; i++) await null;
  log("twoAsyncDrained=" + seen.join(","));

  // 5) a listener that awaits cannot influence defaultPrevented afterwards
  const t5 = new EventTarget();
  t5.addEventListener("c", async function (ev: Event) { await null; ev.preventDefault(); });
  const ev5 = new Event("c", { cancelable: true });
  const r5 = t5.dispatchEvent(ev5);
  log("beforeAwaitPrevent=" + r5 + "," + ev5.defaultPrevented);
  for (let i = 0; i < 3; i++) await null;
  log("afterAwaitPrevent=" + ev5.defaultPrevented);

  // 6) a microtask that dispatches while ANOTHER dispatch is already on the
  //    stack cannot happen -- the queue is only drained between jobs, so the
  //    two dispatches never interleave
  seen.length = 0;
  const t6 = new EventTarget();
  t6.addEventListener("i", function () {
    seen.push("i-start");
    queueMicrotask(function () { seen.push("i-micro"); });
    seen.push("i-end");
  });
  t6.dispatchEvent(new Event("i"));
  t6.dispatchEvent(new Event("i"));
  log("noInterleave=" + seen.join(","));
  for (let i = 0; i < 3; i++) await null;
  log("noInterleaveDrained=" + seen.join(","));

  // 7) an abort listener is an ordinary listener: the abort() call is
  //    synchronous and the microtasks it queues wait
  seen.length = 0;
  const ctl = new AbortController();
  ctl.signal.addEventListener("abort", function () {
    seen.push("abort-listener");
    queueMicrotask(function () { seen.push("abort-micro"); });
  });
  seen.push("beforeAbort");
  ctl.abort();
  seen.push("afterAbort");
  log("abortIsSynchronous=" + seen.join(","));
  await null;
  log("abortDrained=" + seen.join(","));

  // 8) a listener that awaits a settled promise resumes after every remaining
  //    listener of the same dispatch
  seen.length = 0;
  const t8 = new EventTarget();
  t8.addEventListener("k", async function () { seen.push("l1-sync"); await Promise.resolve(); seen.push("l1-resumed"); });
  t8.addEventListener("k", async function () { seen.push("l2-sync"); await Promise.resolve(); seen.push("l2-resumed"); });
  t8.addEventListener("k", function () { seen.push("l3-sync"); });
  t8.dispatchEvent(new Event("k"));
  for (let i = 0; i < 3; i++) await null;
  log("resumeAfterDispatch=" + seen.join(","));

  console.log("end");
})().catch(function () { console.log("UNEXPECTED"); });
