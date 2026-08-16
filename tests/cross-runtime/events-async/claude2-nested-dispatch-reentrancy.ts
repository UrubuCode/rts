// Cross-runtime: dispatching an event from INSIDE a listener. The inner
// dispatch runs to completion before the outer one continues, each dispatch has
// its own stop flags, and a re-entrant dispatch of the same type is allowed.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const seen: string[] = [];
function reset() { seen.length = 0; }

// 1) an inner dispatch of a DIFFERENT type completes before the outer resumes
const t1 = new EventTarget();
t1.addEventListener("outer", function () { seen.push("outer1-start"); t1.dispatchEvent(new Event("inner")); seen.push("outer1-end"); });
t1.addEventListener("outer", function () { seen.push("outer2"); });
t1.addEventListener("inner", function () { seen.push("inner1"); });
t1.addEventListener("inner", function () { seen.push("inner2"); });
reset(); t1.dispatchEvent(new Event("outer"));
log("nestedOrder=" + seen.join(","));

// 2) a re-entrant dispatch of the SAME type, guarded so it stops
const t2 = new EventTarget();
let depth = 0;
t2.addEventListener("re", function () {
  depth++;
  seen.push("enter" + depth);
  if (depth < 3) t2.dispatchEvent(new Event("re"));
  seen.push("leave" + depth);
  depth--;
});
reset(); t2.dispatchEvent(new Event("re"));
log("reentrant=" + seen.join(","));

// 3) stopImmediatePropagation inside the INNER dispatch does not stop the outer
const t3 = new EventTarget();
t3.addEventListener("o", function () { seen.push("o1"); t3.dispatchEvent(new Event("i")); });
t3.addEventListener("o", function () { seen.push("o2"); });
t3.addEventListener("i", function (ev: Event) { seen.push("i1"); ev.stopImmediatePropagation(); });
t3.addEventListener("i", function () { seen.push("i2"); });
reset(); t3.dispatchEvent(new Event("i"));
log("innerAlone=" + seen.join(","));
reset(); t3.dispatchEvent(new Event("o"));
log("innerStopDoesNotLeak=" + seen.join(","));

// 4) each Event object has its own flags: the outer one is untouched
const t4 = new EventTarget();
const outerEv = new Event("o4", { cancelable: true });
let innerPrevented = "unset";
t4.addEventListener("o4", function () {
  const innerEv = new Event("i4", { cancelable: true });
  t4.dispatchEvent(innerEv);
  innerPrevented = String(innerEv.defaultPrevented);
});
t4.addEventListener("i4", function (ev: Event) { ev.preventDefault(); });
const outerResult = t4.dispatchEvent(outerEv);
log("outerUntouched=" + outerEv.defaultPrevented + " dispatchReturned=" + outerResult + " innerPrevented=" + innerPrevented);

// 5) a listener that dispatches to a DIFFERENT target
const src = new EventTarget();
const dst = new EventTarget();
src.addEventListener("go", function (ev: Event) {
  seen.push("srcSees=" + (ev.currentTarget === src));
  dst.dispatchEvent(new Event("relay"));
  seen.push("afterRelay=" + (ev.currentTarget === src));
});
dst.addEventListener("relay", function (ev: Event) { seen.push("dstSees=" + (ev.currentTarget === dst)); });
reset(); src.dispatchEvent(new Event("go"));
log("crossTarget=" + seen.join(","));

// 6) removing the listener that is currently running does not stop it, and it
//    does not run on the next dispatch
const t6 = new EventTarget();
const selfRemoving = function () { seen.push("self"); t6.removeEventListener("s", selfRemoving); seen.push("selfAfterRemove"); };
t6.addEventListener("s", selfRemoving);
t6.addEventListener("s", function () { seen.push("other"); });
reset(); t6.dispatchEvent(new Event("s"));
log("selfRemovalFirst=" + seen.join(","));
reset(); t6.dispatchEvent(new Event("s"));
log("selfRemovalSecond=" + seen.join(","));

// 7) a listener added during an inner dispatch is visible to a LATER dispatch
//    of the outer type only
const t7 = new EventTarget();
const added = function () { seen.push("added"); };
t7.addEventListener("a", function () { seen.push("a1"); t7.dispatchEvent(new Event("b")); });
t7.addEventListener("b", function () { seen.push("b1"); t7.addEventListener("a", added); });
reset(); t7.dispatchEvent(new Event("a"));
log("firstOuter=" + seen.join(","));
reset(); t7.dispatchEvent(new Event("a"));
log("secondOuter=" + seen.join(","));

// 8) dispatch is synchronous all the way down: nothing after it is deferred
const t8 = new EventTarget();
t8.addEventListener("z", function () { seen.push("z"); });
reset();
seen.push("before");
t8.dispatchEvent(new Event("z"));
seen.push("after");
log("fullySynchronous=" + seen.join(","));

// 9) a `once` listener that dispatches the same type again does not re-enter
//    itself: it was unregistered before it ran
const t9 = new EventTarget();
let onceCalls = 0;
t9.addEventListener("o9", function () {
  onceCalls++;
  if (onceCalls < 5) t9.dispatchEvent(new Event("o9"));
}, { once: true });
t9.dispatchEvent(new Event("o9"));
log("onceNoReentry=" + onceCalls);

// 10) the OUTER event keeps its own currentTarget and eventPhase across an
//     inner dispatch at a different target
const outerT = new EventTarget();
const innerT = new EventTarget();
let snapshot = "unset";
outerT.addEventListener("x", function (ev: Event) {
  const before = ev.eventPhase + ":" + (ev.currentTarget === outerT);
  innerT.dispatchEvent(new Event("y"));
  snapshot = "before=" + before + " after=" + ev.eventPhase + ":" + (ev.currentTarget === outerT);
});
innerT.addEventListener("y", function () { });
outerT.dispatchEvent(new Event("x"));
log("outerFieldsIntact=" + snapshot);

// 11) an inner dispatch of an event that is ALREADY being dispatched throws
const t11 = new EventTarget();
let reuse = "unset";
const shared = new Event("s11");
t11.addEventListener("s11", function (ev: Event) {
  try { t11.dispatchEvent(ev); reuse = "no-throw"; } catch (e: any) { reuse = e.constructor.name; }
});
t11.dispatchEvent(shared);
log("reDispatchInFlight=" + reuse);

// 12) after that dispatch ends, the same object may NOT be dispatched again
//     either -- the object is spent only while in flight, so this one works
log("reDispatchAfterwards=" + (function () {
  try { return "returned:" + t11.dispatchEvent(shared); } catch (e: any) { return e.constructor.name; }
})());

// 13) three levels deep, each level completing before its parent resumes
const t13 = new EventTarget();
t13.addEventListener("L1", function () { seen.push("L1-in"); t13.dispatchEvent(new Event("L2")); seen.push("L1-out"); });
t13.addEventListener("L2", function () { seen.push("L2-in"); t13.dispatchEvent(new Event("L3")); seen.push("L2-out"); });
t13.addEventListener("L3", function () { seen.push("L3"); });
reset(); t13.dispatchEvent(new Event("L1"));
log("threeLevels=" + seen.join(","));

console.log("end");
