// Cross-runtime: CustomEvent's `detail`, the Event init flags, and what
// preventDefault does on a cancelable event versus a plain one -- including
// the boolean dispatchEvent hands back.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

// 1) a default-constructed Event
const e1 = new Event("plain");
log("type=" + e1.type);
log("bubbles=" + e1.bubbles + " cancelable=" + e1.cancelable + " composed=" + e1.composed);
log("defaultPrevented=" + e1.defaultPrevented);
log("phase=" + e1.eventPhase + " target=" + String(e1.target) + " currentTarget=" + String(e1.currentTarget));
log("isTrusted=" + e1.isTrusted);
log("ctorName=" + e1.constructor.name);
log("tag=" + Object.prototype.toString.call(e1));

// 2) the init dictionary
const e2 = new Event("flagged", { bubbles: true, cancelable: true, composed: true });
log("flagged=" + e2.bubbles + "," + e2.cancelable + "," + e2.composed);

// 3) preventDefault on a NON-cancelable event does nothing
const t = new EventTarget();
const e3 = new Event("a", { cancelable: false });
t.addEventListener("a", function (ev: Event) { ev.preventDefault(); });
const ok3 = t.dispatchEvent(e3);
log("nonCancelable prevented=" + e3.defaultPrevented + " dispatchReturned=" + ok3);

// 4) preventDefault on a cancelable event flips the flag and the return value
const e4 = new Event("b", { cancelable: true });
t.addEventListener("b", function (ev: Event) { ev.preventDefault(); });
const ok4 = t.dispatchEvent(e4);
log("cancelable prevented=" + e4.defaultPrevented + " dispatchReturned=" + ok4);

// 5) returning false from a listener does NOT cancel anything
const e5 = new Event("c", { cancelable: true });
t.addEventListener("c", function () { return false; } as any);
const ok5 = t.dispatchEvent(e5);
log("returnFalse prevented=" + e5.defaultPrevented + " dispatchReturned=" + ok5);

// 6) CustomEvent carries `detail`, defaulting to null
const c1 = new CustomEvent("d");
log("detailDefault=" + String(c1.detail));
log("customIsEvent=" + (c1 instanceof Event));
log("customProto=" + (Object.getPrototypeOf(CustomEvent.prototype) === Event.prototype));

// 7) detail is passed BY REFERENCE, not cloned
const payload = { k: 1, nested: { deep: true } };
const c2 = new CustomEvent("e", { detail: payload, cancelable: true });
log("detailSameObject=" + (c2.detail === payload));
log("detailK=" + c2.detail.k + " deep=" + c2.detail.nested.deep);

// 8) a listener mutating detail is seen by the dispatcher
t.addEventListener("e", function (ev: any) { ev.detail.k = 99; });
t.dispatchEvent(c2);
log("mutatedDetail=" + payload.k);

// 9) detail may be any value, including a primitive or undefined
log("detailString=" + new CustomEvent("f", { detail: "s" }).detail);
log("detailZero=" + new CustomEvent("f", { detail: 0 }).detail);
log("detailUndefined=" + String(new CustomEvent("f", { detail: undefined }).detail));

// 10) the type is stringified
log("numericType=" + JSON.stringify(new Event(1 as any).type));
log("symbolFreeType=" + JSON.stringify(new Event(null as any).type));

// 11) dispatching a non-Event throws
log("dispatchPlainObject=" + (function () {
  try { (t as any).dispatchEvent({ type: "z" }); return "no"; } catch (e: any) { return e.constructor.name; }
})());

// 12) constructing an Event with no type throws
log("noType=" + (function () {
  try { new (Event as any)(); return "no"; } catch (e: any) { return e.constructor.name; }
})());

console.log("end");
