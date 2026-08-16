// Pins ValidateAndApplyPropertyDescriptor on a NON-CONFIGURABLE property: a
// redefinition with an identical descriptor is accepted, writable true->false
// is the one allowed downgrade, and everything else throws. The existing
// defineProperty fixtures never redefine a locked property.

function shape(target: any, key: string): string {
  const d = Object.getOwnPropertyDescriptor(target, key) as any;
  if (d === undefined) return "undefined";
  if ("get" in d) return "acc,e=" + d.enumerable + ",c=" + d.configurable;
  return "data,v=" + String(d.value) + ",w=" + d.writable + ",e=" + d.enumerable + ",c=" + d.configurable;
}

function attempt(label: string, fn: () => void): void {
  try {
    fn();
    console.log(label + "=ok");
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

const o: any = {};
Object.defineProperty(o, "locked", { value: 1, writable: true, enumerable: true, configurable: false });
console.log("start=" + shape(o, "locked"));

// identical descriptor: accepted, no change
attempt("same", () => Object.defineProperty(o, "locked", { value: 1, writable: true, enumerable: true, configurable: false }));
console.log("after_same=" + shape(o, "locked"));

// a partial descriptor that mentions nothing conflicting is accepted
attempt("empty", () => Object.defineProperty(o, "locked", {}));

// still writable, so a NEW value is allowed
attempt("newvalue", () => Object.defineProperty(o, "locked", { value: 2 }));
console.log("after_newvalue=" + shape(o, "locked"));

// enumerable cannot change while non-configurable
attempt("enum_flip", () => Object.defineProperty(o, "locked", { enumerable: false }));
// configurable cannot be turned back on
attempt("conf_on", () => Object.defineProperty(o, "locked", { configurable: true }));
// data cannot become accessor
attempt("to_accessor", () => Object.defineProperty(o, "locked", { get() { return 0; } }));

// writable true -> false IS allowed
attempt("w_down", () => Object.defineProperty(o, "locked", { writable: false }));
console.log("after_w_down=" + shape(o, "locked"));

// and now it is frozen: no new value, and no way back to writable
attempt("value_after_lock", () => Object.defineProperty(o, "locked", { value: 3 }));
attempt("same_value_after_lock", () => Object.defineProperty(o, "locked", { value: 2 }));
attempt("w_up", () => Object.defineProperty(o, "locked", { writable: true }));
console.log("final=" + shape(o, "locked"));

// Reflect.defineProperty answers false where Object.defineProperty throws
console.log("reflect_conf_on=" + Reflect.defineProperty(o, "locked", { configurable: true }));
console.log("reflect_same=" + Reflect.defineProperty(o, "locked", { value: 2 }));

// a non-configurable ACCESSOR: the same get/set functions are accepted
const g = function () { return "G"; };
const s = function () { /* ignored */ };
const acc: any = {};
Object.defineProperty(acc, "a", { get: g, set: s, enumerable: false, configurable: false });
console.log("acc_start=" + shape(acc, "a"));
attempt("acc_same", () => Object.defineProperty(acc, "a", { get: g, set: s }));
attempt("acc_other_get", () => Object.defineProperty(acc, "a", { get: function () { return "H"; } }));
attempt("acc_drop_set", () => Object.defineProperty(acc, "a", { get: g, set: undefined }));
attempt("acc_to_data", () => Object.defineProperty(acc, "a", { value: 1 }));
console.log("acc_read=" + acc.a);

// deleting a non-configurable property fails
console.log("reflect_delete=" + Reflect.deleteProperty(o, "locked"));
console.log("still_there=" + ("locked" in o));

// -0 and NaN in SameValue terms: the "identical value" test is SameValue
const nz: any = {};
Object.defineProperty(nz, "n", { value: NaN, writable: false, configurable: false });
attempt("nan_same", () => Object.defineProperty(nz, "n", { value: NaN }));
Object.defineProperty(nz, "z", { value: 0, writable: false, configurable: false });
attempt("zero_same", () => Object.defineProperty(nz, "z", { value: 0 }));
attempt("zero_neg", () => Object.defineProperty(nz, "z", { value: -0 }));
