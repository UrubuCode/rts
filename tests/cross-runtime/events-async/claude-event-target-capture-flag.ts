// Cross-runtime: the CAPTURE flag is part of a listener's IDENTITY -- the same
// function registered with and without it is two entries, and the de-duplication
// rule compares that flag. Also the AT_TARGET phase fields during a dispatch.
//
// Two neighbouring behaviours are deliberately NOT asserted here because Bun and
// Node genuinely disagree: `removeEventListener(type, fn, true)` with a BOOLEAN
// third argument (Node keeps the capturing entry, Bun removes it), and the
// relative order of capturing vs non-capturing listeners at AT_TARGET.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

let seen: string[] = [];
function reset() { seen = []; }

// 1) one function, two entries: capture and bubble both fire
const t1 = new EventTarget();
const f = function () { seen.push("f"); };
t1.addEventListener("p", f, true);
t1.addEventListener("p", f, false);
reset(); t1.dispatchEvent(new Event("p"));
log("bothEntriesCount=" + seen.length);

// 2) removing without the flag removes only the non-capturing entry
t1.removeEventListener("p", f);
reset(); t1.dispatchEvent(new Event("p"));
log("afterRemoveBubble=" + seen.join(","));

// 3) the capturing entry goes when removed with a matching options object
t1.removeEventListener("p", f, { capture: true });
reset(); t1.dispatchEvent(new Event("p"));
log("afterRemoveCapture=" + JSON.stringify(seen.join(",")));

// 4) adding with `true` and then with `{ capture: true }` is a DUPLICATE
const t2 = new EventTarget();
const g = function () { seen.push("g"); };
t2.addEventListener("q", g, true);
t2.addEventListener("q", g, { capture: true });
reset(); t2.dispatchEvent(new Event("q"));
log("booleanEqualsOptions=" + seen.join(","));
t2.removeEventListener("q", g, { capture: true });
reset(); t2.dispatchEvent(new Event("q"));
log("removedByOptions=" + JSON.stringify(seen.join(",")));

// 5) `false`, `undefined`, `{}` and `{ capture: false }` are all one entry
const t3 = new EventTarget();
const h = function () { seen.push("h"); };
t3.addEventListener("r", h, false);
t3.addEventListener("r", h, undefined);
t3.addEventListener("r", h, {});
t3.addEventListener("r", h, { capture: false });
reset(); t3.dispatchEvent(new Event("r"));
log("allNonCapturing=" + seen.join(","));
t3.removeEventListener("r", h);
reset(); t3.dispatchEvent(new Event("r"));
log("oneRemoveClearsThem=" + JSON.stringify(seen.join(",")));

// 6) both entries of one function see the SAME event object
const t4 = new EventTarget();
const objs: any[] = [];
const collect = function (ev: Event) { objs.push(ev); };
t4.addEventListener("s", collect, { capture: true });
t4.addEventListener("s", collect, { capture: false });
t4.dispatchEvent(new Event("s"));
log("sameEventObject=" + (objs.length === 2 && objs[0] === objs[1]));

// 7) the AT_TARGET fields during a dispatch
const t5 = new EventTarget();
const phases: string[] = [];
t5.addEventListener("u", function (ev: Event) {
  phases.push("phase=" + ev.eventPhase);
  phases.push("atTarget=" + (ev.eventPhase === Event.AT_TARGET));
  phases.push("target=" + (ev.target === t5));
  phases.push("current=" + (ev.currentTarget === t5));
}, true);
t5.dispatchEvent(new Event("u"));
log("atTarget=" + phases.join("|"));

// 8) the phase constants, on the instance and on the constructor
log("NONE=" + Event.NONE + " CAPTURING=" + Event.CAPTURING_PHASE + " AT_TARGET=" + Event.AT_TARGET + " BUBBLING=" + Event.BUBBLING_PHASE);
log("constantsAreNumbers=" + (typeof Event.AT_TARGET) + "," + (typeof Event.NONE));

// 9) currentTarget and phase reset once the dispatch is over
const t6 = new EventTarget();
let captured: any = null;
t6.addEventListener("v", function (ev: Event) { captured = ev; });
t6.dispatchEvent(new Event("v"));
log("afterCurrentTarget=" + String(captured.currentTarget));
log("afterTarget=" + (captured.target === t6));
log("afterPhase=" + captured.eventPhase);

// 10) once + capture together unregisters the CAPTURING entry only
const t7 = new EventTarget();
const both = function () { seen.push("both"); };
t7.addEventListener("w", both, { capture: true, once: true });
t7.addEventListener("w", both, { capture: false });
reset(); t7.dispatchEvent(new Event("w"));
log("onceCaptureFirst=" + seen.join(","));
reset(); t7.dispatchEvent(new Event("w"));
log("onceCaptureSecond=" + seen.join(","));

// 11) a passive option changes nothing about whether the listener runs
const t8 = new EventTarget();
t8.addEventListener("x", function () { seen.push("passive"); }, { passive: true });
reset(); t8.dispatchEvent(new Event("x"));
log("passive=" + seen.join(","));

// 12) an options object is read at ADD time, not kept live
const t9 = new EventTarget();
const opts: any = { capture: false };
t9.addEventListener("y", function () { seen.push("y"); }, opts);
opts.capture = true;
t9.removeEventListener("y", function () { }, { capture: false });
reset(); t9.dispatchEvent(new Event("y"));
log("optionsSnapshot=" + seen.join(","));

console.log("end");
