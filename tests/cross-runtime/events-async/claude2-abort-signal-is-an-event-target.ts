// Cross-runtime: an AbortSignal is an ordinary EventTarget. Dispatching an
// "abort" event at it BY HAND runs the listeners but does not make it aborted --
// only the controller does that -- and listener options work as everywhere else.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const seen: string[] = [];

// 1) a hand-dispatched "abort" event reaches the listeners
const c1 = new AbortController();
c1.signal.addEventListener("abort", function (ev: Event) {
  seen.push("listener type=" + ev.type + " targetIsSignal=" + (ev.target === c1.signal));
});
const fake = new Event("abort");
const dispatched = c1.signal.dispatchEvent(fake);
log("dispatchReturned=" + dispatched);
log("saw=" + seen.join(" | "));

// 2) but the signal is NOT aborted by that, and the reason is still undefined
log("abortedAfterFakeEvent=" + c1.signal.aborted + " reason=" + String(c1.signal.reason));
log("throwIfAbortedStillQuiet=" + (function () {
  try { c1.signal.throwIfAborted(); return "no-throw"; } catch (e: any) { return e.constructor.name; }
})());

// 3) the real abort fires the listener a SECOND time
c1.abort("real");
log("afterRealAbort=" + c1.signal.aborted + " reason=" + c1.signal.reason + " listenerCalls=" + seen.length);

// 4) a hand-dispatched event AFTER the abort still reaches listeners: the
//    "already aborted, never fire again" rule is about the abort algorithm,
//    not about dispatchEvent
seen.length = 0;
c1.signal.dispatchEvent(new Event("abort"));
log("dispatchAfterAbort=" + seen.length);

// 5) `once` on a signal listener behaves as it does on any target
const c5 = new AbortController();
let onceCalls = 0;
c5.signal.addEventListener("abort", function () { onceCalls++; }, { once: true });
c5.signal.dispatchEvent(new Event("abort"));
c5.abort();
log("onceCalls=" + onceCalls + " aborted=" + c5.signal.aborted);

// 6) removeEventListener unregisters an abort listener before the abort
const c6 = new AbortController();
let removedCalls = 0;
const fn6 = function () { removedCalls++; };
c6.signal.addEventListener("abort", fn6);
c6.signal.removeEventListener("abort", fn6);
c6.abort();
log("removedCalls=" + removedCalls + " aborted=" + c6.signal.aborted);

// 7) listeners for OTHER types on a signal work too
const c7 = new AbortController();
let customCalls = 0;
c7.signal.addEventListener("custom", function () { customCalls++; });
c7.signal.dispatchEvent(new Event("custom"));
c7.abort();
log("customCalls=" + customCalls);

// 8) several abort listeners run in registration order, once each
const c8 = new AbortController();
const order8: string[] = [];
c8.signal.addEventListener("abort", function () { order8.push("first"); });
c8.signal.addEventListener("abort", function () { order8.push("second"); });
c8.signal.onabort = function () { order8.push("onabort"); };
c8.signal.addEventListener("abort", function () { order8.push("third"); });
c8.abort();
log("abortListenerOrder=" + order8.join(","));

// 9) assigning onabort twice keeps only the last handler
const c9 = new AbortController();
const order9: string[] = [];
c9.signal.onabort = function () { order9.push("A"); };
c9.signal.onabort = function () { order9.push("B"); };
log("onabortIsLast=" + (typeof c9.signal.onabort));
c9.abort();
log("onabortReplaced=" + order9.join(","));

// 10) setting onabort to null clears it
const c10 = new AbortController();
let cleared = 0;
c10.signal.onabort = function () { cleared++; };
c10.signal.onabort = null;
c10.abort();
log("onabortCleared=" + cleared + " value=" + String(c10.signal.onabort));

// 11) the abort event that the CONTROLLER fires: its shape
const c11 = new AbortController();
let shape = "unset";
c11.signal.addEventListener("abort", function (ev: Event) {
  shape = "type=" + ev.type + " bubbles=" + ev.bubbles + " cancelable=" + ev.cancelable +
    " isEvent=" + (ev instanceof Event) + " currentTargetIsSignal=" + (ev.currentTarget === c11.signal) +
    " phase=" + ev.eventPhase;
});
c11.abort();
log("realAbortEvent=" + shape);

// 12) a signal from the static helper is aborted from the start and never
//     fires, however it is poked
const s12 = AbortSignal.abort("already");
let staticCalls = 0;
s12.addEventListener("abort", function () { staticCalls++; });
log("staticAborted=" + s12.aborted + " reason=" + s12.reason + " listenerCalls=" + staticCalls);
s12.dispatchEvent(new Event("abort"));
log("staticAfterManualDispatch=" + staticCalls + " stillAborted=" + s12.aborted);

console.log("end");
