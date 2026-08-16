// Pins the defineProperty trap: what the descriptor ARGUMENT looks like (a
// fresh plain object carrying only the fields that were written), and the three
// invariants — no new key on a non-extensible target, no non-configurable
// definition for a key the target lacks, and none over a configurable one.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

// the trap sees exactly the fields present in the descriptor it was given
let seen: any = null;
const spy: any = new Proxy({} as any, {
  defineProperty(t, k, d) { seen = d; return Reflect.defineProperty(t, k, d); },
});

Reflect.defineProperty(spy, "a", { value: 1 });
console.log("fields_value=" + Reflect.ownKeys(seen).join("|"));
Reflect.defineProperty(spy, "b", { value: 2, writable: true, enumerable: true, configurable: true });
console.log("fields_full=" + Reflect.ownKeys(seen).sort().join("|"));
Reflect.defineProperty(spy, "c", { get() { return 3; }, configurable: true });
console.log("fields_accessor=" + Reflect.ownKeys(seen).sort().join("|"));
Reflect.defineProperty(spy, "d", { enumerable: false });
console.log("fields_lonely=" + Reflect.ownKeys(seen).join("|"));
console.log("desc_is_plain=" + (Object.getPrototypeOf(seen) === Object.prototype));
console.log("desc_extensible=" + Object.isExtensible(seen));
console.log("desc_types=" + typeof seen.enumerable);

// the attributes are COERCED before the trap sees them
Reflect.defineProperty(spy, "e", { value: 5, enumerable: 1 as any, configurable: "" as any });
console.log("coerced=e=" + seen.enumerable + ",c=" + seen.configurable);

// a plain assignment through a proxy with no set trap reaches defineProperty on
// the RECEIVER, and that receiver is the proxy
const assignSeen: string[] = [];
const assignProxy: any = new Proxy({} as any, {
  defineProperty(t, k, d) { assignSeen.push(String(k) + ":" + Reflect.ownKeys(d).sort().join("+")); return Reflect.defineProperty(t, k, d); },
});
Reflect.set(assignProxy, "fresh", 1);
console.log("assign_define=" + assignSeen.join(","));

// refusal: Reflect answers false, Object throws, and the target is untouched
const refusedTarget: any = {};
const refuses: any = new Proxy(refusedTarget, { defineProperty() { return false; } });
console.log("refuse_reflect=" + Reflect.defineProperty(refuses, "x", { value: 1 }));
attempt("refuse_object", () => { Object.defineProperty(refuses, "x", { value: 1 }); return "ok"; });
console.log("refuse_target=" + Object.getOwnPropertyNames(refusedTarget).length);
// a truthy non-boolean report counts as success
const truthy: any = new Proxy({} as any, { defineProperty(t, k, d) { Reflect.defineProperty(t, k, d); return 1 as any; } });
console.log("truthy_ok=" + Reflect.defineProperty(truthy, "y", { value: 2, configurable: true }));
const emptyString: any = new Proxy({} as any, { defineProperty(t, k, d) { Reflect.defineProperty(t, k, d); return "" as any; } });
console.log("falsy_report=" + Reflect.defineProperty(emptyString, "y", { value: 2, configurable: true }));

// invariant 1: nothing new may be added to a non-extensible target
const shut: any = Object.preventExtensions({ old: 1 });
const shutProxy: any = new Proxy(shut, { defineProperty() { return true; } });
attempt("nonext_new", () => String(Reflect.defineProperty(shutProxy, "brand", { value: 1, configurable: true })));
console.log("nonext_existing=" + Reflect.defineProperty(shutProxy, "old", { value: 9, configurable: true }));
console.log("nonext_target_untouched=" + shut.old);

// invariant 2: a non-configurable definition needs the key on the target
const emptyTarget: any = {};
const inventor: any = new Proxy(emptyTarget, { defineProperty() { return true; } });
attempt("invent_nonconf", () => String(Reflect.defineProperty(inventor, "ghost", { value: 1, configurable: false })));
console.log("invent_conf=" + Reflect.defineProperty(inventor, "ghost", { value: 1, configurable: true }));
console.log("invent_target=" + Object.getOwnPropertyNames(emptyTarget).length);

// invariant 3: not over a key the target has as CONFIGURABLE either
const looseTarget: any = { loose: 1 };
const upgrader: any = new Proxy(looseTarget, { defineProperty() { return true; } });
attempt("upgrade_nonconf", () => String(Reflect.defineProperty(upgrader, "loose", { value: 1, configurable: false })));

// but it is legal once the target really holds a non-configurable slot
const fixedTarget: any = {};
Object.defineProperty(fixedTarget, "fixed", { value: 1, writable: false, enumerable: true, configurable: false });
const honest: any = new Proxy(fixedTarget, { defineProperty() { return true; } });
console.log("honest_nonconf=" + Reflect.defineProperty(honest, "fixed", { value: 1, writable: false, enumerable: true, configurable: false }));
// and refused when the reported definition is incompatible with that slot
attempt("incompatible", () => String(Reflect.defineProperty(honest, "fixed", { value: 2, configurable: false })));
console.log("fixed_value=" + fixedTarget.fixed);
