// Pins getPrototypeOf/setPrototypeOf on a proxy: over an EXTENSIBLE target the
// traps may answer anything (the target is never consulted), over a
// non-extensible one they must match it exactly, and the refusal is a boolean
// through Reflect but a throw through Object.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

const protoA: any = { tag: "A" };
const protoB: any = { tag: "B" };

// over an extensible target the trap is free, and the target does not move
const openTarget: any = Object.create(protoA);
const lying: any = new Proxy(openTarget, { getPrototypeOf() { return protoB; } });
console.log("open_lie=" + (Object.getPrototypeOf(lying) === protoB));
console.log("open_target_intact=" + (Object.getPrototypeOf(openTarget) === protoA));
// but the lie does NOT drive a plain read: with no get trap the read forwards
// to the TARGET's [[Get]], which walks the target's own chain
console.log("open_lookup=" + lying.tag);
console.log("open_instanceof=" + (lying instanceof Object));

// the trap result must be an object or null; anything else is a TypeError
console.log("null_proto=" + String(Object.getPrototypeOf(new Proxy({}, { getPrototypeOf() { return null; } }))));
attempt("proto_number", () => String(Object.getPrototypeOf(new Proxy({}, { getPrototypeOf() { return 1 as any; } }))));
attempt("proto_string", () => String(Object.getPrototypeOf(new Proxy({}, { getPrototypeOf() { return "p" as any; } }))));
attempt("proto_undefined", () => String(Object.getPrototypeOf(new Proxy({}, { getPrototypeOf() { return undefined as any; } }))));
console.log("proto_function=" + (typeof Object.getPrototypeOf(new Proxy({}, { getPrototypeOf() { return protoA; } }))));

// over a NON-EXTENSIBLE target the trap must report the real prototype
const shut: any = Object.create(protoA);
Object.preventExtensions(shut);
console.log("shut_honest=" + (Object.getPrototypeOf(new Proxy(shut, { getPrototypeOf() { return protoA; } })) === protoA));
attempt("shut_lie", () => String(Object.getPrototypeOf(new Proxy(shut, { getPrototypeOf() { return protoB; } }))));
attempt("shut_null", () => String(Object.getPrototypeOf(new Proxy(shut, { getPrototypeOf() { return null; } }))));

// setPrototypeOf: refusing is always legal, and only the report differs
const refuses: any = new Proxy({}, { setPrototypeOf() { return false; } });
console.log("set_refuse_reflect=" + Reflect.setPrototypeOf(refuses, protoA));
attempt("set_refuse_object", () => { Object.setPrototypeOf(refuses, protoA); return "ok"; });

// a trap that reports success without moving the target is legal while the
// target is extensible — the proxy simply lies
const idleTarget: any = {};
const idle: any = new Proxy(idleTarget, { setPrototypeOf() { return true; } });
console.log("set_idle=" + Reflect.setPrototypeOf(idle, protoA));
console.log("set_idle_target=" + (Object.getPrototypeOf(idleTarget) === Object.prototype));
console.log("set_idle_read=" + (Object.getPrototypeOf(idle) === Object.prototype));

// over a non-extensible target it is not: the value must equal the current one
const shut2: any = Object.create(protoA);
Object.preventExtensions(shut2);
const shutProxy: any = new Proxy(shut2, { setPrototypeOf() { return true; } });
attempt("shut_set_other", () => String(Reflect.setPrototypeOf(shutProxy, protoB)));
console.log("shut_set_same=" + Reflect.setPrototypeOf(shutProxy, protoA));

// forwarding traps behave exactly like no trap at all
const fwdTarget: any = {};
const fwd: any = new Proxy(fwdTarget, {
  getPrototypeOf(t) { return Reflect.getPrototypeOf(t); },
  setPrototypeOf(t, v) { return Reflect.setPrototypeOf(t, v); },
});
console.log("fwd_set=" + Reflect.setPrototypeOf(fwd, protoA));
console.log("fwd_target_moved=" + (Object.getPrototypeOf(fwdTarget) === protoA));
console.log("fwd_read=" + fwd.tag);
console.log("fwd_null=" + Reflect.setPrototypeOf(fwd, null));
console.log("fwd_after_null=" + String(Object.getPrototypeOf(fwd)));

// __proto__ is an accessor on Object.prototype, so it reaches the traps only
// when the proxy still inherits from Object.prototype
const dunder: any = new Proxy({} as any, {
  getPrototypeOf() { return protoB; },
  setPrototypeOf() { return false; },
});
console.log("dunder_get=" + (dunder.__proto__ === protoB));
// the setter turns a refused [[SetPrototypeOf]] into a throw of its own
attempt("dunder_set", () => String(Reflect.set(dunder, "__proto__", protoA)));

// the trap is asked once per question, not cached
let calls = 0;
const counted: any = new Proxy({}, { getPrototypeOf() { calls++; return protoA; } });
Object.getPrototypeOf(counted);
Object.getPrototypeOf(counted);
void counted.tag;
console.log("proto_calls=" + calls);
