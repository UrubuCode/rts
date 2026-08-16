// Cross-runtime: `addEventListener(type, fn, { signal })` ties a listener to an
// AbortSignal -- aborting removes it, an ALREADY-aborted signal means the
// listener is never registered at all, and the option composes with `once`.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const seen: string[] = [];
function reset() { seen.length = 0; }

// 1) a listener registered with a signal fires normally until the abort
const t1 = new EventTarget();
const c1 = new AbortController();
t1.addEventListener("e", function () { seen.push("tied"); }, { signal: c1.signal });
t1.addEventListener("e", function () { seen.push("plain"); });
reset(); t1.dispatchEvent(new Event("e"));
log("beforeAbort=" + seen.join(","));
c1.abort();
reset(); t1.dispatchEvent(new Event("e"));
log("afterAbort=" + seen.join(","));

// 2) an already-aborted signal registers nothing
const t2 = new EventTarget();
t2.addEventListener("e", function () { seen.push("never"); }, { signal: AbortSignal.abort() });
t2.addEventListener("e", function () { seen.push("survivor"); });
reset(); t2.dispatchEvent(new Event("e"));
log("alreadyAborted=" + seen.join(","));

// 3) the signal option combines with capture: the pair is still one identity
const t3 = new EventTarget();
const c3 = new AbortController();
const fn3 = function () { seen.push("cap"); };
t3.addEventListener("e", fn3, { signal: c3.signal, capture: true });
t3.addEventListener("e", fn3, { signal: c3.signal });
reset(); t3.dispatchEvent(new Event("e"));
log("captureAndBubblePhase=" + seen.join(","));
c3.abort();
reset(); t3.dispatchEvent(new Event("e"));
log("abortRemovesBoth=" + JSON.stringify(seen.join(",")));

// 4) `once` plus `signal`: whichever comes first ends the registration
const t4 = new EventTarget();
const c4 = new AbortController();
let onceCalls = 0;
t4.addEventListener("e", function () { onceCalls++; }, { signal: c4.signal, once: true });
t4.dispatchEvent(new Event("e"));
t4.dispatchEvent(new Event("e"));
c4.abort();
t4.dispatchEvent(new Event("e"));
log("onceWithSignal=" + onceCalls);

// 5) one signal can retire many listeners on many targets at once
const ta = new EventTarget();
const tb = new EventTarget();
const c5 = new AbortController();
ta.addEventListener("e", function () { seen.push("a1"); }, { signal: c5.signal });
ta.addEventListener("e", function () { seen.push("a2"); }, { signal: c5.signal });
tb.addEventListener("e", function () { seen.push("b1"); }, { signal: c5.signal });
tb.addEventListener("e", function () { seen.push("b2"); });
reset(); ta.dispatchEvent(new Event("e")); tb.dispatchEvent(new Event("e"));
log("manyBefore=" + seen.join(","));
c5.abort();
reset(); ta.dispatchEvent(new Event("e")); tb.dispatchEvent(new Event("e"));
log("manyAfter=" + seen.join(","));

// 6) removeEventListener still works on a signal-tied listener, and a later
//    abort is then a no-op
const t6 = new EventTarget();
const c6 = new AbortController();
const fn6 = function () { seen.push("f6"); };
t6.addEventListener("e", fn6, { signal: c6.signal });
t6.removeEventListener("e", fn6);
reset(); t6.dispatchEvent(new Event("e"));
log("removedManually=" + JSON.stringify(seen.join(",")));
c6.abort();
reset(); t6.dispatchEvent(new Event("e"));
log("abortAfterRemoval=" + JSON.stringify(seen.join(",")));

// 7) re-adding after the abort with a FRESH signal works
const t7 = new EventTarget();
const c7a = new AbortController();
const c7b = new AbortController();
const fn7 = function () { seen.push("f7"); };
t7.addEventListener("e", fn7, { signal: c7a.signal });
c7a.abort();
t7.addEventListener("e", fn7, { signal: c7b.signal });
reset(); t7.dispatchEvent(new Event("e"));
log("reAdded=" + seen.join(","));
c7b.abort();
reset(); t7.dispatchEvent(new Event("e"));
log("secondSignalAborts=" + JSON.stringify(seen.join(",")));

// 8) a signal that is not an AbortSignal is a TypeError
const t8 = new EventTarget();
log("badSignal=" + (function () {
  try { t8.addEventListener("e", function () { }, { signal: {} } as any); return "no"; }
  catch (e: any) { return e.constructor.name; }
})());
log("nullSignalIgnored=" + (function () {
  try { t8.addEventListener("e", function () { seen.push("nullSig"); }, { signal: null } as any); return "accepted"; }
  catch (e: any) { return e.constructor.name; }
})());
reset(); t8.dispatchEvent(new Event("e"));
log("nullSignalListener=" + seen.join(","));

// 9) aborting fires the signal's own abort listeners too, and the removal has
//    already happened by the time a later dispatch runs
const t9 = new EventTarget();
const c9 = new AbortController();
c9.signal.addEventListener("abort", function () { seen.push("signal-abort"); });
t9.addEventListener("e", function () { seen.push("tied9"); }, { signal: c9.signal });
reset();
c9.abort();
t9.dispatchEvent(new Event("e"));
log("orderOnAbort=" + seen.join(",") + " aborted=" + c9.signal.aborted);

console.log("end");
