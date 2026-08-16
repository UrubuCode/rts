// Pins the per-trap invariants around a NON-CONFIGURABLE target property: `get`
// must return the same value for a non-writable data slot, `has` cannot hide it,
// `getOwnPropertyDescriptor` cannot report undefined for it, and a getterless
// accessor must be reported as undefined.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

const t: any = {};
Object.defineProperty(t, "frozen", { value: "REAL", writable: false, configurable: false, enumerable: true });
Object.defineProperty(t, "roAcc", { get: undefined, set: undefined, configurable: false, enumerable: true });
Object.defineProperty(t, "writable", { value: "W", writable: true, configurable: false, enumerable: true });
t.loose = "L";

// get must agree on a non-configurable non-writable data property
attempt("get_lie", () => String(new Proxy(t, { get() { return "FAKE"; } }).frozen));
attempt("get_same", () => String(new Proxy(t, { get() { return "REAL"; } }).frozen));
// a non-configurable but WRITABLE property may be reported differently
attempt("get_writable_lie", () => String(new Proxy(t, { get() { return "FAKE"; } }).writable));
// a configurable one is free
attempt("get_loose_lie", () => String(new Proxy(t, { get() { return "FAKE"; } }).loose));
// a non-configurable accessor with no getter must report undefined
attempt("get_acc_lie", () => String(new Proxy(t, { get() { return "FAKE"; } }).roAcc));
attempt("get_acc_undef", () => String(new Proxy(t, { get() { return undefined; } }).roAcc));

// has cannot hide a non-configurable property
attempt("has_hide_nc", () => String("frozen" in new Proxy(t, { has() { return false; } })));
attempt("has_hide_loose", () => String("loose" in new Proxy(t, { has() { return false; } })));
// nor a property of a NON-EXTENSIBLE target
const nx: any = { k: 1 };
Object.preventExtensions(nx);
attempt("has_hide_nonext", () => String("k" in new Proxy(nx, { has() { return false; } })));
// but it may invent one freely
attempt("has_invent", () => String("nope" in new Proxy(t, { has() { return true; } })));

// getOwnPropertyDescriptor cannot report undefined for a non-configurable key
attempt("gopd_hide_nc", () => String(Object.getOwnPropertyDescriptor(new Proxy(t, { getOwnPropertyDescriptor() { return undefined; } }), "frozen")));
attempt("gopd_hide_loose", () => String(Object.getOwnPropertyDescriptor(new Proxy(t, { getOwnPropertyDescriptor() { return undefined; } }), "loose")));
// nor claim a configurable descriptor for a non-configurable property
attempt("gopd_conf_lie", () => {
  const px = new Proxy(t, { getOwnPropertyDescriptor() { return { value: "REAL", configurable: true, writable: false, enumerable: true }; } });
  const d = Object.getOwnPropertyDescriptor(px, "frozen") as any;
  return "c=" + d.configurable;
});
// nor invent a non-configurable descriptor for a property the target lacks
attempt("gopd_invent_nc", () => {
  const px = new Proxy({} as any, { getOwnPropertyDescriptor() { return { value: 1, configurable: false, writable: true, enumerable: true }; } });
  const d = Object.getOwnPropertyDescriptor(px, "any") as any;
  return "c=" + d.configurable;
});
// an invented CONFIGURABLE descriptor is fine and is normalised to a full one
attempt("gopd_invent_ok", () => {
  const px = new Proxy({} as any, { getOwnPropertyDescriptor() { return { value: 1, configurable: true }; } });
  const d = Object.getOwnPropertyDescriptor(px, "any") as any;
  return "v=" + d.value + ",w=" + d.writable + ",e=" + d.enumerable + ",c=" + d.configurable;
});
// a non-object, non-undefined trap result is rejected
attempt("gopd_nonobject", () => String(Object.getOwnPropertyDescriptor(new Proxy({} as any, { getOwnPropertyDescriptor() { return 1 as any; } }), "x")));

// defineProperty answering false: Object.defineProperty throws, Reflect returns it
const refuse = new Proxy({} as any, { defineProperty() { return false; } });
attempt("define_object", () => { Object.defineProperty(refuse, "x", { value: 1 }); return "ok"; });
attempt("define_reflect", () => String(Reflect.defineProperty(refuse, "x", { value: 1 })));

// deleteProperty cannot remove a non-configurable property even if it says true
attempt("delete_lie", () => String(Reflect.deleteProperty(new Proxy(t, { deleteProperty() { return true; } }), "frozen")));
attempt("delete_loose", () => String(Reflect.deleteProperty(new Proxy({ z: 1 } as any, { deleteProperty() { return true; } }), "z")));

// setPrototypeOf on a non-extensible target must match the real prototype
const base = { tag: "base" };
const fixed: any = Object.create(base);
Object.preventExtensions(fixed);
attempt("setproto_other", () => String(Reflect.setPrototypeOf(new Proxy(fixed, { setPrototypeOf() { return true; } }), { tag: "other" })));
attempt("setproto_same", () => String(Reflect.setPrototypeOf(new Proxy(fixed, { setPrototypeOf() { return true; } }), base)));
// getPrototypeOf must not lie about a non-extensible target either
attempt("getproto_lie", () => String(Object.getPrototypeOf(new Proxy(fixed, { getPrototypeOf() { return null; } }))));
attempt("getproto_true", () => String(Object.getPrototypeOf(new Proxy(fixed, { getPrototypeOf() { return base; } })) === base));

// isExtensible must agree with the target
attempt("isext_lie", () => String(Reflect.isExtensible(new Proxy(fixed, { isExtensible() { return true; } }))));
attempt("isext_true", () => String(Reflect.isExtensible(new Proxy(fixed, { isExtensible() { return false; } }))));
