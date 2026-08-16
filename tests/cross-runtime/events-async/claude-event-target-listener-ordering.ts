// Cross-runtime: EventTarget listener bookkeeping -- registration order, the
// de-duplication rule, `once`, and what adding/removing DURING a dispatch does
// to the listener list of that same dispatch.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const t = new EventTarget();
let seen: string[] = [];
function reset() { seen = []; }
function fire(type: string) { return t.dispatchEvent(new Event(type)); }

// 1) listeners run in registration order
const a = function () { seen.push("a"); };
const b = function () { seen.push("b"); };
const c = function () { seen.push("c"); };
t.addEventListener("p", a);
t.addEventListener("p", b);
t.addEventListener("p", c);
reset(); fire("p");
log("order=" + seen.join(","));

// 2) the SAME function added again for the same type/capture is ignored
t.addEventListener("p", a);
reset(); fire("p");
log("afterDuplicate=" + seen.join(","));

// 3) removing one keeps the rest in order
t.removeEventListener("p", b);
reset(); fire("p");
log("afterRemove=" + seen.join(","));

// 4) removing something never added is silently fine
t.removeEventListener("p", function () { });
t.removeEventListener("nosuch", a);
reset(); fire("p");
log("afterNoopRemove=" + seen.join(","));

// 5) a listener added DURING a dispatch does not run in that dispatch
const late = function () { seen.push("late"); };
const adder = function () { seen.push("adder"); t.addEventListener("p", late); };
t.addEventListener("p", adder);
reset(); fire("p");
log("addDuringDispatch=" + seen.join(","));
reset(); fire("p");
log("nextDispatch=" + seen.join(","));
t.removeEventListener("p", adder);
t.removeEventListener("p", late);

// 6) a listener removed DURING a dispatch does not run in that dispatch, even
//    though it was in the list when the dispatch began
const tr = new EventTarget();
const victim = function () { seen.push("victim"); };
tr.addEventListener("p", function () { seen.push("remover"); tr.removeEventListener("p", victim); });
tr.addEventListener("p", victim);
tr.addEventListener("p", function () { seen.push("survivor"); });
reset(); tr.dispatchEvent(new Event("p"));
log("removeDuringDispatch=" + seen.join(","));

// 7) `once` fires exactly once and then unregisters itself
const only = function () { seen.push("once"); };
t.addEventListener("q", only, { once: true });
t.addEventListener("q", function () { seen.push("plain"); });
reset(); fire("q");
log("onceFirst=" + seen.join(","));
reset(); fire("q");
log("onceSecond=" + seen.join(","));

// 8) stopImmediatePropagation halts the rest of the list on the same target
const t2 = new EventTarget();
t2.addEventListener("r", function () { seen.push("r1"); });
t2.addEventListener("r", function (ev: Event) { seen.push("r2"); ev.stopImmediatePropagation(); });
t2.addEventListener("r", function () { seen.push("r3"); });
reset();
const ok8 = t2.dispatchEvent(new Event("r"));
log("stopImmediate=" + seen.join(",") + " dispatchReturned=" + ok8);

// 9) stopPropagation alone does NOT stop the remaining listeners here
const t3 = new EventTarget();
t3.addEventListener("s", function (ev: Event) { seen.push("s1"); ev.stopPropagation(); });
t3.addEventListener("s", function () { seen.push("s2"); });
reset(); t3.dispatchEvent(new Event("s"));
log("stopPropagation=" + seen.join(","));

// 10) dispatching a type nobody listens for is a no-op that still returns true
//     (a null listener would do the same, but both runtimes print a warning to
//     stderr for it, so it is deliberately not exercised here)
const t4 = new EventTarget();
t4.addEventListener("u", function () { seen.push("u1"); });
reset();
const okUnknown = t4.dispatchEvent(new Event("unlistened"));
log("unknownType=" + okUnknown + " seen=" + JSON.stringify(seen.join(",")));
reset(); t4.dispatchEvent(new Event("u"));
log("knownType=" + seen.join(","));

// 11) an object with handleEvent is a valid listener
const t5 = new EventTarget();
const handler = { calls: 0, handleEvent: function (ev: Event) { seen.push("handleEvent:" + ev.type); (this as any).calls++; } };
t5.addEventListener("v", handler as any);
reset(); t5.dispatchEvent(new Event("v"));
log("handleEvent=" + seen.join(",") + " calls=" + handler.calls);

// 12) dispatch is SYNCHRONOUS: nothing is deferred to a microtask
const t6 = new EventTarget();
reset();
t6.addEventListener("w", function () { seen.push("inListener"); });
t6.dispatchEvent(new Event("w"));
seen.push("afterDispatch");
log("synchronous=" + seen.join(","));

// 13) dispatching the same event object twice throws
const t7 = new EventTarget();
const ev7 = new Event("x");
t7.dispatchEvent(ev7);
log("reuseDispatch=" + (function () { try { t7.dispatchEvent(ev7); return "no"; } catch (e: any) { return e.constructor.name; } })());

console.log("end");
