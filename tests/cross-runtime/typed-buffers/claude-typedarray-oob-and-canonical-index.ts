// Cross-runtime: an out-of-range index on a typed array is not an ordinary
// property. Writes are silently dropped, reads answer undefined, and a key that
// is not a CANONICAL numeric string never becomes an own property either.

const t = new Uint8Array([10, 20, 30]);

// Out-of-bounds write: dropped, and no property created. Probed through
// Reflect.set, which REPORTS the [[Set]] result instead of turning it into a
// throw — the bare assignment form would answer differently in sloppy and
// strict code and so would measure the caller's mode, not the typed array.
console.log("oob_set_result=" + Reflect.set(t, "5", 42) + "," + Reflect.set(t, "-1", 42));
console.log("oob_write_read=" + String(t[5]) + "," + String(t[-1]));
console.log("oob_in=" + ("5" in t) + "," + ("-1" in t));
console.log("oob_keys=" + Object.keys(t).join(","));
console.log("oob_len=" + t.length);
console.log("oob_hasOwn=" + Object.prototype.hasOwnProperty.call(t, "5"));
console.log("oob_desc=" + String(Object.getOwnPropertyDescriptor(t, "5")));

// The one place a bare assignment is safe to test: this function carries its
// own "use strict" directive, so it is strict whether the file is loaded as a
// module or as a script. The write is still a silent no-op, because [[Set]]
// answers true for an out-of-range index and strict mode only throws on false.
const strictWrite = function (): string {
  "use strict";
  try {
    t[9] = 1;
    return "no-throw";
  } catch (e: any) {
    return e.constructor.name;
  }
};
console.log("oob_strict=" + strictWrite());

// In-bounds keys ARE own properties, and they are not configurable.
const d0 = Object.getOwnPropertyDescriptor(t, "0") as any;
console.log("in_desc=w:" + d0.writable + " e:" + d0.enumerable + " c:" + d0.configurable);
console.log("in_value=" + d0.value);

// Non-canonical numeric strings are ordinary string keys and DO stick, while a
// canonical one that is out of range is dropped. Both are set through
// Reflect.set for the same reason as above: the answer is a boolean here and a
// mode-dependent throw with a bare assignment.
const n = new Uint8Array(3);
const keys: string[] = ["1.5", "-0", "+1", "01", "1e1", " 1", "0x1"];
console.log("canon_set_results=" + keys.map(function (k) {
  return k + ":" + Reflect.set(n, k, 9);
}).join(" "));
console.log("canon_elems=" + Array.from(n).join(","));
console.log("canon_keys=" + Object.keys(n).join("|"));
console.log("canon_read_1.5=" + String((n as any)["1.5"]));
console.log("canon_read_-0=" + String((n as any)["-0"]));
console.log("canon_read_01=" + String((n as any)["01"]));
console.log("canon_read_+1=" + String((n as any)["+1"]));
console.log("canon_desc_1.5=" + String(Object.getOwnPropertyDescriptor(n, "1.5")));
console.log("canon_desc_01=" + typeof Object.getOwnPropertyDescriptor(n, "01"));

// "Infinity" and "NaN" are canonical numeric strings, so they are refused.
const inf = new Uint8Array(2);
console.log("inf_set_results=" + ["Infinity", "NaN", "-Infinity"].map(function (k) {
  return k + ":" + Reflect.set(inf, k, 9);
}).join(" "));
console.log("inf_keys=" + Object.keys(inf).join("|"));
console.log("inf_read=" + String((inf as any)["Infinity"]) + "," + String((inf as any)["NaN"]));

// defineProperty cannot reach out of bounds, and cannot reshape an element.
try {
  Object.defineProperty(n, "9", { value: 1 });
  console.log("def_oob=no-throw");
} catch (e: any) {
  console.log("def_oob=" + e.constructor.name);
}
try {
  Object.defineProperty(n, "0", { get: function () { return 1; } });
  console.log("def_accessor=no-throw");
} catch (e: any) {
  console.log("def_accessor=" + e.constructor.name);
}
try {
  Object.defineProperty(n, "0", { value: 5, configurable: false, writable: true, enumerable: true });
  console.log("def_value=ok:" + n[0]);
} catch (e: any) {
  console.log("def_value=" + e.constructor.name);
}

// [[Delete]] succeeds for anything that is not an in-range index, and refuses
// one that is. Reflect.deleteProperty reports the answer; the `delete` operator
// would throw on the refusal in strict code and answer false in sloppy code.
console.log("delete_oob=" + Reflect.deleteProperty(n, "99"));
console.log("delete_in_range=" + Reflect.deleteProperty(n, "0") + " value=" + n[0]);
console.log("delete_str=" + Reflect.deleteProperty(n, "01") + " keys=" + Object.keys(n).join("|"));

// A non-empty typed array cannot be frozen or sealed into place.
try {
  Object.freeze(new Uint8Array(1));
  console.log("freeze=ok");
} catch (e: any) {
  console.log("freeze=" + e.constructor.name);
}
console.log("freeze_empty=" + (Object.isFrozen(Object.freeze(new Uint8Array(0)))));
console.log("extensible=" + Object.isExtensible(new Uint8Array(1)));
const sealed = new Uint8Array([1, 2]);
Object.preventExtensions(sealed);
console.log("prevent_then_set=" + Reflect.set(sealed, "0", 9) + " oob=" + Reflect.set(sealed, "9", 1));
console.log("after_prevent=" + Array.from(sealed).join(",") + " ext=" + Object.isExtensible(sealed));

// Ordinary string keys survive preventExtensions being off, indexes still win.
const withProp: any = new Uint8Array([1]);
withProp.tag = "x";
console.log("ownKeys=" + Reflect.ownKeys(withProp).join("|"));
console.log("forin=" + (function (): string {
  const acc: string[] = [];
  for (const k in withProp) acc.push(k);
  return acc.join("|");
})());
