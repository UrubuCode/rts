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

// `Object.groupBy(items, cb)` — ES2024 (routed here by `objstatic.rs`). Groups
// `items` into a fresh plain object keyed by the PROPERTY KEY of `cb(item, index)`
// (string-coerced), buckets in first-seen key order. Parallel proven arrays keep
// the buckets pushable; the dynamic `out[key] = bucket` writes append via the
// runtime shape transition.
function __object_group_by(items: any[], cb: any): any {
  const keys: string[] = [];
  const buckets: any[][] = [];
  for (let i = 0; i < items.length; i++) {
    const k = "" + cb(items[i], i);
    let found = false;
    for (let j = 0; j < keys.length; j++) {
      if (keys[j] === k) { buckets[j].push(items[i]); found = true; break; }
    }
    if (!found) { keys.push(k); buckets.push([items[i]]); }
  }
  const out: any = {};
  for (let j = 0; j < keys.length; j++) { out[keys[j]] = buckets[j]; }
  return out;
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
