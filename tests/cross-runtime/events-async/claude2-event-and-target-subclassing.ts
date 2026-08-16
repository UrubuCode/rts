// Cross-runtime: subclassing Event and EventTarget. Focus: a subclass instance
// survives dispatch unchanged, the init dictionary's defaults, where the Event
// fields actually LIVE (accessors on the prototype), and composedPath.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

// 1) an EventTarget subclass is a working target and keeps its own state
class Bus extends EventTarget {
  name: string;
  count: number;
  constructor(name: string) { super(); this.name = name; this.count = 0; }
  emit(type: string, detail: any) {
    this.count++;
    return this.dispatchEvent(new Signal(type, detail));
  }
}

// 2) an Event subclass carries its own fields
class Signal extends Event {
  detail: any;
  constructor(type: string, detail: any) { super(type, { cancelable: true }); this.detail = detail; }
}

const bus = new Bus("main");
const seen: string[] = [];

bus.addEventListener("ping", function (ev: any) {
  seen.push("type=" + ev.type);
  seen.push("detail=" + ev.detail);
  seen.push("isSignal=" + (ev instanceof Signal));
  seen.push("isEvent=" + (ev instanceof Event));
  seen.push("targetIsBus=" + (ev.target === bus));
  seen.push("currentTargetIsBus=" + (ev.currentTarget === bus));
  seen.push("busName=" + (ev.target as any).name);
});

const ok = bus.emit("ping", "P1");
log("dispatchReturned=" + ok + " count=" + bus.count);
log("listenerSaw=" + seen.join(" "));

// 3) the subclass instance is the SAME object before and after dispatch
const sig = new Signal("keep", "K");
let inside: any = null;
bus.addEventListener("keep", function (ev: any) { inside = ev; });
bus.dispatchEvent(sig);
log("sameObject=" + (inside === sig) + " detailAfter=" + sig.detail);
log("protoChain=" + (Object.getPrototypeOf(sig) === Signal.prototype) + "," + (Object.getPrototypeOf(Signal.prototype) === Event.prototype));

// 4) `super(type, init)` defaults: everything false unless asked for
const bare = new Event("bare");
log("bareFlags=" + [bare.bubbles, bare.cancelable, bare.composed, bare.defaultPrevented].join(","));
const full = new Event("full", { bubbles: true, cancelable: true, composed: true });
log("fullFlags=" + [full.bubbles, full.cancelable, full.composed].join(","));

// 5) the init dictionary is read for its properties, so a getter is honoured
const viaGetter = new Event("g", { get bubbles() { return true; } } as any);
log("initGetter=" + viaGetter.bubbles);

// 6) unknown init keys are ignored, and a missing type is a TypeError
const extra = new Event("x", { bubbles: true, nonsense: 1 } as any);
log("extraIgnored=" + extra.bubbles + " hasNonsense=" + ("nonsense" in extra));
log("noType=" + (function () {
  try { new (Event as any)(); return "no"; } catch (e: any) { return e.constructor.name; }
})());

// 7) the fields are ACCESSORS on Event.prototype, not own data properties
log("typeIsOwn=" + Object.prototype.hasOwnProperty.call(bare, "type"));
const desc: any = Object.getOwnPropertyDescriptor(Event.prototype, "type");
log("typeDescriptor=" + typeof desc.get + "," + typeof desc.set + "," + desc.enumerable + "," + desc.configurable);
log("detailIsOwn=" + Object.prototype.hasOwnProperty.call(sig, "detail"));

// 8) the string tags
log("eventTag=" + Object.prototype.toString.call(bare));
log("signalTag=" + Object.prototype.toString.call(sig));
log("targetTag=" + Object.prototype.toString.call(bus));

// 9) composedPath on a lone target: [target] during dispatch, [] outside it
let pathInside = "unset";
const t9 = new EventTarget();
t9.addEventListener("c", function (ev: Event) {
  const p = ev.composedPath();
  pathInside = "len=" + p.length + " first=" + (p[0] === t9);
});
const ev9 = new Event("c");
log("pathBefore=" + ev9.composedPath().length);
t9.dispatchEvent(ev9);
log("pathDuring=" + pathInside);
log("pathAfter=" + ev9.composedPath().length);

// 10) preventDefault only bites on a cancelable event, and dispatchEvent
//     reports it
const t10 = new EventTarget();
t10.addEventListener("p", function (ev: Event) { ev.preventDefault(); });
const cancelable = new Event("p", { cancelable: true });
const plain = new Event("p");
log("cancelableDispatch=" + t10.dispatchEvent(cancelable) + " defaultPrevented=" + cancelable.defaultPrevented);
log("plainDispatch=" + t10.dispatchEvent(plain) + " defaultPrevented=" + plain.defaultPrevented);

// 11) a Bus with no listener for the type still returns true and counts
log("noListener=" + bus.emit("silent", "S") + " count=" + bus.count);

// 12) EventTarget.prototype methods are not own properties of an instance
log("addIsInherited=" + (Object.prototype.hasOwnProperty.call(bus, "addEventListener") === false) +
  " reachable=" + (typeof bus.addEventListener));
log("busIsEventTarget=" + (bus instanceof EventTarget) + " ctorName=" + bus.constructor.name);

console.log("end");
