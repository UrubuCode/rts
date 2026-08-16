// Pins Object.freeze/seal/isFrozen/isSealed over a PROXY: they are not traps of
// their own, they are spelled out in terms of preventExtensions, ownKeys,
// getOwnPropertyDescriptor and defineProperty — so a handler can make freeze
// throw, and isFrozen can answer true for a target that is not frozen at all.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

// there is no freeze/seal/isFrozen trap
const names = ["freeze", "seal", "isFrozen", "isSealed", "keys", "assign"];
const unknownTraps: string[] = [];
const sniff: any = new Proxy({ a: 1 }, new Proxy({} as any, {
  get(_t, k) { unknownTraps.push(String(k)); return undefined; },
}));
Object.freeze(sniff);
console.log("freeze_traps=" + unknownTraps.join("|"));
console.log("no_such_traps=" + names.map((n) => n + ":" + (unknownTraps.indexOf(n) < 0)).join("|"));

// a defineProperty trap that refuses makes Object.freeze throw, mid-way
const applied: string[] = [];
const halfTarget: any = { a: 1, b: 2, c: 3 };
const half: any = new Proxy(halfTarget, {
  defineProperty(t, k, d) {
    if (k === "b") return false;
    applied.push(String(k));
    return Reflect.defineProperty(t, k, d);
  },
});
attempt("freeze_refused", () => { Object.freeze(half); return "ok"; });
console.log("applied=" + applied.join("|"));
console.log("target_a=" + (Object.getOwnPropertyDescriptor(halfTarget, "a") as any).writable);
console.log("target_b=" + (Object.getOwnPropertyDescriptor(halfTarget, "b") as any).writable);
console.log("target_c=" + (Object.getOwnPropertyDescriptor(halfTarget, "c") as any).writable);
console.log("target_extensible=" + Object.isExtensible(halfTarget));

// isFrozen reads the handler's descriptors, but the gopd invariant refuses to
// let it be fooled: claiming non-configurable for a configurable target
// property throws instead of answering true
const lyingTarget: any = { x: 1 };
Object.preventExtensions(lyingTarget);
const handler: any = {
  isExtensible() { return false; },
  preventExtensions(t: any) { Object.preventExtensions(t); return true; },
  getOwnPropertyDescriptor() { return { value: 1, writable: false, enumerable: true, configurable: false }; },
  ownKeys() { return ["x"]; },
};
const lying: any = new Proxy(lyingTarget, handler);
attempt("lying_isFrozen", () => String(Object.isFrozen(lying)));
attempt("lying_isSealed", () => String(Object.isSealed(lying)));
console.log("target_really_frozen=" + Object.isFrozen(lyingTarget));
console.log("target_x_writable=" + (Object.getOwnPropertyDescriptor(lyingTarget, "x") as any).writable);

// once the target genuinely matches what the handler claims, the same handler
// is legal and the answer is true
const honestFrozen: any = {};
Object.defineProperty(honestFrozen, "x", { value: 1, writable: false, enumerable: true, configurable: false });
Object.preventExtensions(honestFrozen);
const honest: any = new Proxy(honestFrozen, handler);
console.log("honest_isFrozen=" + Object.isFrozen(honest));
console.log("honest_isSealed=" + Object.isSealed(honest));

// and the other way: a frozen target behind a handler that reports it loose
const frozenTarget: any = Object.freeze({ y: 1 });
const loose: any = new Proxy(frozenTarget, {
  getOwnPropertyDescriptor() { return { value: 1, writable: true, enumerable: true, configurable: true }; },
});
attempt("loose_isFrozen", () => String(Object.isFrozen(loose)));

// preventExtensions must actually make the target non-extensible
const fakePrevent: any = new Proxy({ z: 1 }, { preventExtensions() { return true; } });
attempt("fake_prevent", () => String(Reflect.preventExtensions(fakePrevent)));
const honestTarget: any = { z: 1 };
const honestPrevent: any = new Proxy(honestTarget, {
  preventExtensions(t) { Object.preventExtensions(t); return true; },
});
console.log("honest_prevent=" + Reflect.preventExtensions(honestPrevent) + ",target=" + Object.isExtensible(honestTarget));

// a trap that answers false: Object.preventExtensions throws, Reflect returns it
const refusing: any = new Proxy({}, { preventExtensions() { return false; } });
attempt("obj_prevent_refused", () => { Object.preventExtensions(refusing); return "ok"; });
console.log("ref_prevent_refused=" + Reflect.preventExtensions(refusing));

// isExtensible must agree with the target, in both directions
const extTarget: any = {};
console.log("isext_true=" + Reflect.isExtensible(new Proxy(extTarget, { isExtensible() { return true; } })));
attempt("isext_false_lie", () => String(Reflect.isExtensible(new Proxy(extTarget, { isExtensible() { return false; } }))));

// the trap result is coerced with ToBoolean before the invariant check
console.log("isext_truthy=" + Reflect.isExtensible(new Proxy(extTarget, { isExtensible() { return 1 as any; } })));
console.log("isext_string=" + Reflect.isExtensible(new Proxy(extTarget, { isExtensible() { return "no" as any; } })));

// a proxy over a FROZEN target still answers frozen without any handler
const plainFrozen: any = new Proxy(Object.freeze({ k: 1 }), {});
console.log("plain_frozen=" + Object.isFrozen(plainFrozen) + "," + Object.isSealed(plainFrozen));
console.log("plain_ext=" + Object.isExtensible(plainFrozen));

// sealing a proxy of an EMPTY target only needs preventExtensions
const emptyLog: string[] = [];
const emptyProxy: any = new Proxy({}, {
  ownKeys(t) { emptyLog.push("ownKeys"); return Reflect.ownKeys(t); },
  preventExtensions(t) { emptyLog.push("preventExtensions"); Object.preventExtensions(t); return true; },
  defineProperty(t, k, d) { emptyLog.push("define:" + String(k)); return Reflect.defineProperty(t, k, d); },
});
Object.seal(emptyProxy);
console.log("empty_seal=" + emptyLog.join("|"));
console.log("empty_isSealed=" + Object.isSealed(emptyProxy));

// a revoked proxy answers nothing about its integrity
const rev = Proxy.revocable({ a: 1 }, {});
rev.revoke();
attempt("revoked_isFrozen", () => String(Object.isFrozen(rev.proxy)));
attempt("revoked_freeze", () => { Object.freeze(rev.proxy); return "ok"; });
attempt("revoked_isExtensible", () => String(Object.isExtensible(rev.proxy)));
