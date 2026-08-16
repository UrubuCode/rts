// Cross-runtime: what `abort()` puts in `signal.reason`, when the abort event
// fires, and `throwIfAborted`. Focus: the default reason is an AbortError, a
// second abort is a no-op, and a listener added AFTER the abort never runs.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const seen: string[] = [];

// 1) a fresh controller
const c1 = new AbortController();
log("beforeAborted=" + c1.signal.aborted);
log("beforeReason=" + String(c1.signal.reason));
log("signalCtor=" + c1.signal.constructor.name);
log("controllerCtor=" + c1.constructor.name);
log("signalIsEventTarget=" + (c1.signal instanceof EventTarget));

// 2) throwIfAborted before the abort is a no-op
log("throwIfBefore=" + (function () {
  try { c1.signal.throwIfAborted(); return "no"; } catch (e: any) { return e.name; }
})());

// 3) abort() with no reason: an AbortError DOMException
c1.signal.addEventListener("abort", function (ev: Event) {
  seen.push("listener:" + ev.type + ":" + (ev.target === c1.signal));
});
c1.abort();
log("afterAborted=" + c1.signal.aborted);
log("reasonName=" + c1.signal.reason.name);
log("reasonCtor=" + c1.signal.reason.constructor.name);
log("reasonIsError=" + (c1.signal.reason instanceof Error));
log("listenerRan=" + seen.join(","));

// 4) the abort event fired SYNCHRONOUSLY inside abort()
log("synchronousAbort=" + (seen.length === 1));

// 5) throwIfAborted now throws the reason itself
let thrown: any = null;
try { c1.signal.throwIfAborted(); } catch (e: any) { thrown = e; }
log("throwIfAfter=" + (thrown === c1.signal.reason) + " name=" + thrown.name);

// 6) a second abort is a no-op: the reason is unchanged and nothing fires
seen.length = 0;
c1.abort("ignored");
log("secondAbortReason=" + c1.signal.reason.name);
log("secondAbortFired=" + JSON.stringify(seen.join(",")));

// 7) a listener added AFTER the abort never runs
c1.signal.addEventListener("abort", function () { seen.push("late"); });
log("lateListener=" + JSON.stringify(seen.join(",")));

// 8) an explicit reason is kept verbatim, whatever it is
const c2 = new AbortController();
const token = { why: "stop" };
c2.abort(token);
log("customSame=" + (c2.signal.reason === token));
log("customWhy=" + c2.signal.reason.why);

const c3 = new AbortController();
c3.abort(undefined);
log("undefinedReasonName=" + c3.signal.reason.name);

const c4 = new AbortController();
c4.abort(null);
log("nullReason=" + String(c4.signal.reason));

const c5 = new AbortController();
c5.abort(0);
log("zeroReason=" + c5.signal.reason);

// 9) AbortSignal.abort() builds an already-aborted signal with no controller
const s6 = AbortSignal.abort();
log("staticAborted=" + s6.aborted + " name=" + s6.reason.name);
const s7 = AbortSignal.abort("why");
log("staticWithReason=" + s7.aborted + " reason=" + s7.reason);
s7.addEventListener("abort", function () { seen.push("staticLate"); });
log("staticLateListener=" + JSON.stringify(seen.join(",")));

// 10) onabort is the property form of the same listener slot
const c8 = new AbortController();
c8.signal.onabort = function () { seen.push("onabort"); };
log("onabortIsFunction=" + (typeof c8.signal.onabort));
c8.abort();
log("onabortRan=" + seen.join(","));

// 11) the abort event object is a plain Event
const c9 = new AbortController();
let ev9: any = null;
c9.signal.addEventListener("abort", function (ev: Event) { ev9 = ev; });
c9.abort();
log("eventType=" + ev9.type + " isEvent=" + (ev9 instanceof Event) + " bubbles=" + ev9.bubbles + " cancelable=" + ev9.cancelable);

// 12) aborting is not a microtask: everything above already happened
console.log("end");
