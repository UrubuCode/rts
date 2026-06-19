// Faithful TypeScript `Object` — the REAL primordial Object instance-method
// library + factory for the new engine, written in `.ts` instead of hardcoded in
// codegen. Mirrors the boolean.ts / number.ts / string.ts pattern, adapted to the
// fact that Object is NOT a primitive with an autobox: every `{}` is ALREADY a
// shape-based object, so there is no `__prim` slot and no dual-`this` unwrap — the
// methods read `this` AS THE OBJECT directly.
//
//   Object(value)      → an object (the `ObjectFactory` function)
//   new Object()       → a fresh object (the ambient `class Object`, user-class path)
//   obj.hasOwnProperty(k) / obj.toString() / ...  → routed to this class with the
//      object as `this` (the OBJECT-receiver dispatch in `front/run/method.rs`).
//
// ## The irreducible shape logic stays codegen-side (one source of truth)
// Property presence on a shape object is the engine's OWN concern (the slot-0
// shape-id + the global shape registry), exposed to the prelude via the PRIVATE
// `engine.obj_has(this, key)` (wrapping the codegen `__rtsadp_obj_has`). The
// static surface (`Object.keys`/`values`/`entries`/`assign`/`freeze`) stays
// codegen-native (shape-based) in `front/run/objstatic.rs` — this prelude class is
// INSTANCE-only and is transparent to that static path.

// `Object(value)` — coerce to an object. `null`/`undefined` → a fresh empty
// object; an existing value passes through (primitive→wrapper boxing is a later
// increment).
function ObjectFactory(value?: any): any {
  if (value === undefined || value === null) {
    return {};
  }
  return value;
}

class Object {
  // `obj.hasOwnProperty(key)` — true iff `key` is an own property of `this`,
  // decided by the engine's shape-aware membership check.
  hasOwnProperty(key: string): boolean {
    return engine.obj_has(this, key);
  }
  // In the flat shape model every own property is enumerable, so this matches
  // `hasOwnProperty`.
  propertyIsEnumerable(key: string): boolean {
    return engine.obj_has(this, key);
  }
  // JS `Object.prototype.toString` default tag for a plain object.
  toString(): string {
    return "[object Object]";
  }
  toLocaleString(): string {
    return "[object Object]";
  }
  // `Object.prototype.valueOf` returns the object itself.
  valueOf(): any {
    return this;
  }
}
