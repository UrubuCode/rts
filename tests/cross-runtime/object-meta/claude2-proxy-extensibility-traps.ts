// Pins the two extensibility traps, which are the ones the spec allows NO
// latitude at all: isExtensible must repeat the target's answer, and
// preventExtensions may only report success once the target is really sealed
// shut — a lie in either direction is a TypeError, not a wrong answer.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

// no trap: both forward
const plain: any = new Proxy({}, {});
console.log("forward_ext=" + Reflect.isExtensible(plain));
console.log("forward_prevent=" + Reflect.preventExtensions(plain));
console.log("forward_after=" + Reflect.isExtensible(plain));

// isExtensible: the trap result is ToBoolean'd, then compared with the target
const openTarget: any = {};
console.log("agree_true=" + Reflect.isExtensible(new Proxy(openTarget, { isExtensible() { return true; } })));
console.log("agree_truthy=" + Reflect.isExtensible(new Proxy(openTarget, { isExtensible() { return 1 as any; } })));
console.log("agree_str=" + Reflect.isExtensible(new Proxy(openTarget, { isExtensible() { return "yes" as any; } })));
attempt("lie_false", () => String(Reflect.isExtensible(new Proxy(openTarget, { isExtensible() { return false; } }))));
attempt("lie_zero", () => String(Reflect.isExtensible(new Proxy(openTarget, { isExtensible() { return 0 as any; } }))));
attempt("lie_undefined", () => String(Reflect.isExtensible(new Proxy(openTarget, { isExtensible() { return undefined as any; } }))));

const shutTarget: any = Object.preventExtensions({});
console.log("shut_agree=" + Reflect.isExtensible(new Proxy(shutTarget, { isExtensible() { return false; } })));
attempt("shut_lie", () => String(Reflect.isExtensible(new Proxy(shutTarget, { isExtensible() { return true; } }))));
// Object.isExtensible answers the same way, and throws the same way
console.log("obj_shut=" + Object.isExtensible(new Proxy(shutTarget, {})));
attempt("obj_shut_lie", () => String(Object.isExtensible(new Proxy(shutTarget, { isExtensible() { return true; } }))));

// preventExtensions: reporting true while the target stays extensible throws
const stillOpen: any = {};
attempt("prevent_lie", () => String(Reflect.preventExtensions(new Proxy(stillOpen, { preventExtensions() { return true; } }))));
console.log("stillOpen_extensible=" + Object.isExtensible(stillOpen));

// reporting true after actually sealing the target is legal
const honestTarget: any = {};
console.log("prevent_honest=" + Reflect.preventExtensions(new Proxy(honestTarget, {
  preventExtensions(t) { Object.preventExtensions(t); return true; },
})));
console.log("honest_extensible=" + Object.isExtensible(honestTarget));

// reporting FALSE is always allowed: Reflect answers false, Object throws
const refusing: any = new Proxy({}, { preventExtensions() { return false; } });
console.log("prevent_refuse_reflect=" + Reflect.preventExtensions(refusing));
attempt("prevent_refuse_object", () => { Object.preventExtensions(refusing); return "ok"; });

// a target that is ALREADY non-extensible lets the trap report true for free
const already: any = Object.preventExtensions({});
console.log("already_true=" + Reflect.preventExtensions(new Proxy(already, { preventExtensions() { return true; } })));

// Object.preventExtensions returns the PROXY, not the target
const returnedTarget: any = {};
const returnedProxy: any = new Proxy(returnedTarget, {});
console.log("returns_proxy=" + (Object.preventExtensions(returnedProxy) === returnedProxy));
console.log("returns_not_target=" + (Object.preventExtensions(returnedProxy) === returnedTarget));

// isSealed/isFrozen are spelled with isExtensible first, so a lying trap makes
// them throw rather than answer
const lyingSealed: any = new Proxy(Object.preventExtensions({ a: 1 }), { isExtensible() { return true; } });
attempt("isSealed_lie", () => String(Object.isSealed(lyingSealed)));
attempt("isFrozen_lie", () => String(Object.isFrozen(lyingSealed)));

// once non-extensible, adding through the proxy is refused even with a set trap
const closed: any = Object.preventExtensions({ a: 1 });
const closedProxy: any = new Proxy(closed, { set(t, k, v, r) { return Reflect.set(t, k, v, r); } });
console.log("add_refused=" + Reflect.set(closedProxy, "b", 1));
console.log("update_ok=" + Reflect.set(closedProxy, "a", 2) + ",v=" + closed.a);
console.log("define_refused=" + Reflect.defineProperty(closedProxy, "c", { value: 1, configurable: true }));
console.log("closed_keys=" + Object.keys(closed).join("|"));
