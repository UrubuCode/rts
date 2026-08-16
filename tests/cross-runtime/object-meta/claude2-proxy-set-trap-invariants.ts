// Pins the [[Set]] trap: a refusal is a boolean everywhere it is observable
// (Reflect.set false, Object.assign a throw), a lie about writing a plain slot
// is allowed, and a lie over a non-configurable non-writable slot — or over a
// setterless non-configurable accessor — is a TypeError.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

// a refusing trap: false through Reflect, a throw through the operations that
// use Set(O, P, V, true)
const refusedTarget: any = { a: 1 };
const refuses: any = new Proxy(refusedTarget, { set() { return false; } });
console.log("reflect_set=" + Reflect.set(refuses, "a", 2));
console.log("target_after=" + refusedTarget.a);
attempt("assign_to", () => { Object.assign(refuses, { a: 3 }); return "ok"; });
attempt("define_bypass", () => String(Reflect.defineProperty(refuses, "a", { value: 4 })));
console.log("target_after_define=" + refusedTarget.a);

// the report is ToBoolean'd
const truthy: any = new Proxy({ a: 1 }, { set() { return "yes" as any; } });
console.log("truthy_report=" + Reflect.set(truthy, "a", 2));
const zero: any = new Proxy({ a: 1 }, { set() { return 0 as any; } });
console.log("zero_report=" + Reflect.set(zero, "a", 2));
const nothing: any = new Proxy({ a: 1 }, { set() { /* undefined */ } });
console.log("undefined_report=" + Reflect.set(nothing, "a", 2));

// a trap that claims success without writing is legal for an ordinary slot
const idleTarget: any = { a: 1 };
const idle: any = new Proxy(idleTarget, { set() { return true; } });
console.log("idle_set=" + Reflect.set(idle, "a", 99));
console.log("idle_target=" + idleTarget.a);
console.log("idle_read=" + idle.a);

// but not over a non-configurable, non-writable data property
const frozenTarget: any = {};
Object.defineProperty(frozenTarget, "fixed", { value: "REAL", writable: false, enumerable: true, configurable: false });
Object.defineProperty(frozenTarget, "ncWritable", { value: "W", writable: true, enumerable: true, configurable: false });
frozenTarget.loose = "L";
const liar: any = new Proxy(frozenTarget, { set() { return true; } });
attempt("lie_fixed", () => String(Reflect.set(liar, "fixed", "OTHER")));
console.log("lie_fixed_same=" + Reflect.set(liar, "fixed", "REAL"));
console.log("lie_nc_writable=" + Reflect.set(liar, "ncWritable", "OTHER"));
console.log("lie_loose=" + Reflect.set(liar, "loose", "OTHER"));
console.log("target_fixed=" + frozenTarget.fixed);

// and not over a non-configurable accessor with no setter
const accTarget: any = {};
Object.defineProperty(accTarget, "readOnly", { get() { return 1; }, configurable: false, enumerable: true });
Object.defineProperty(accTarget, "confReadOnly", { get() { return 1; }, configurable: true, enumerable: true });
const accLiar: any = new Proxy(accTarget, { set() { return true; } });
attempt("lie_setterless", () => String(Reflect.set(accLiar, "readOnly", 2)));
console.log("lie_setterless_conf=" + Reflect.set(accLiar, "confReadOnly", 2));

// with NO set trap the write forwards, and a frozen target simply refuses
const frozenPlain: any = Object.freeze({ a: 1 });
const forwarding: any = new Proxy(frozenPlain, {});
console.log("forward_frozen=" + Reflect.set(forwarding, "a", 2));
console.log("forward_new=" + Reflect.set(forwarding, "b", 1));

// the trap receives the receiver, and forwarding it through Reflect.set makes
// the write land on the PROXY (which loops back into defineProperty)
const chainLog: string[] = [];
const chainTarget: any = { a: 1 };
const chain: any = new Proxy(chainTarget, {
  set(t, k, v, r) { chainLog.push("set:" + String(k) + ":recv_is_proxy=" + (r === chain)); return Reflect.set(t, k, v, r); },
  defineProperty(t, k, d) { chainLog.push("define:" + String(k)); return Reflect.defineProperty(t, k, d); },
  getOwnPropertyDescriptor(t, k) { chainLog.push("gopd:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
});
console.log("chain_set=" + Reflect.set(chain, "a", 7));
console.log("chain_log=" + chainLog.join(","));
console.log("chain_target=" + chainTarget.a);

// a set trap on a proxy used as a PROTOTYPE claims the write: the child gets
// no own property when the trap reports success
const protoTarget: any = {};
const protoProxy: any = new Proxy(protoTarget, { set() { return true; } });
const child: any = Object.create(protoProxy);
console.log("child_set=" + Reflect.set(child, "k", 1));
console.log("child_own=" + Object.getOwnPropertyNames(child).join("|"));
console.log("proto_target_own=" + Object.getOwnPropertyNames(protoTarget).join("|"));
