// Reflect — rts-shared stdlib utility (NOT a primordial; no native syntax). Pure
// TS over primordials only: dynamic property access (`target[key]`) + Object.keys.
// The engine NAMES nothing Reflect-specific — `Reflect.get(...)` is an ordinary
// static call on this ambient class.
//
// Why pure TS works for the trap-bearing cases: `target[key]` /
// `target[key] = value` lower through the engine's DYNAMIC property trampolines
// (`__rtsadp_obj_get`/`_set`), which detect a Proxy receiver and fire its
// `get`/`set` trap — so `Reflect.get(proxy, k)` / `Reflect.set(proxy, k, v)`
// observe the trap automatically, no Reflect-side proxy logic.
//
// Phase 1 surface: get / set / has. The descriptor surface
// (defineProperty / getOwnPropertyDescriptor / ownKeys) and the
// prototype/apply/construct reflectors are a later increment.
class Reflect {
  // Reflect.get(target, key[, receiver]) — `receiver` (a getter's `this`) is not
  // modeled; the 2-arg form covers the observed usage.
  static get(target: any, key: any, receiver?: any): any {
    return target[key];
  }

  // Reflect.set(target, key, value[, receiver]) — returns whether the assignment
  // succeeded. The dynamic set never reports failure here, so it is always `true`
  // (a `set` trap that returns `false` is not yet propagated — later increment).
  static set(target: any, key: any, value: any, receiver?: any): any {
    target[key] = value;
    return true;
  }

  // Reflect.has(target, key) — `key in target`. Checked against the target's OWN
  // enumerable keys (the common case); the prototype chain + a Proxy `has` trap
  // are a later increment.
  static has(target: any, key: any): any {
    return Object.keys(target).indexOf(key) >= 0;
  }

  // Reflect.deleteProperty(target, key) — `delete target[key]`; returns whether the
  // property is gone afterward (always true here — the model has no
  // non-configurable own properties).
  static deleteProperty(target: any, key: any): any {
    delete target[key];
    return true;
  }

  // Reflect.ownKeys(target) — the target's own (string) keys. Symbol keys are a
  // later increment (Symbol itself is not yet modeled).
  static ownKeys(target: any): any {
    return Object.keys(target);
  }
}
