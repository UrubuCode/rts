// Pins ToPropertyDescriptor: the fields are read in the SPEC order
// (enumerable, configurable, value, writable, get, set) regardless of the
// descriptor object's own key order, the three flags go through ToBoolean, and
// mixing a data field with an accessor field throws.

const order: string[] = [];

// the descriptor object declares its keys in reverse spec order
const probe: any = {};
Object.defineProperty(probe, "set", { get() { order.push("set"); return undefined; }, enumerable: true });
Object.defineProperty(probe, "get", { get() { order.push("get"); return undefined; }, enumerable: true });
Object.defineProperty(probe, "writable", { get() { order.push("writable"); return true; }, enumerable: true });
Object.defineProperty(probe, "value", { get() { order.push("value"); return 1; }, enumerable: true });
Object.defineProperty(probe, "configurable", { get() { order.push("configurable"); return true; }, enumerable: true });
Object.defineProperty(probe, "enumerable", { get() { order.push("enumerable"); return true; }, enumerable: true });

try {
  Object.defineProperty({} as any, "k", probe);
  console.log("probe=ok");
} catch (e: any) {
  console.log("probe=throw:" + e.constructor.name);
}
console.log("read_order=" + order.join("|"));
console.log("declared_order=" + Object.keys(probe).join("|"));

// the fields are read through the prototype chain too
const inherited: any = Object.create({ value: "FROM_PROTO", enumerable: true });
const target1: any = {};
Object.defineProperty(target1, "k", inherited);
const d1 = Object.getOwnPropertyDescriptor(target1, "k") as any;
console.log("inherited=" + d1.value + ",e=" + d1.enumerable + ",w=" + d1.writable);

// the three flags go through ToBoolean, not a type check
function flags(desc: any): string {
  const t: any = {};
  Object.defineProperty(t, "k", desc);
  const d = Object.getOwnPropertyDescriptor(t, "k") as any;
  return "w=" + d.writable + ",e=" + d.enumerable + ",c=" + d.configurable;
}
console.log("string_false=" + flags({ value: 1, writable: "false", enumerable: "0", configurable: "" }));
console.log("numbers=" + flags({ value: 1, writable: 1, enumerable: 0, configurable: -1 }));
console.log("objects=" + flags({ value: 1, writable: {}, enumerable: [], configurable: null }));
console.log("nan=" + flags({ value: 1, writable: NaN, enumerable: Infinity, configurable: undefined }));

// but a PRESENT-with-undefined flag still counts as present (and coerces to false)
const t2: any = {};
Object.defineProperty(t2, "k", { value: 1, writable: true, enumerable: true, configurable: true });
Object.defineProperty(t2, "k", { enumerable: undefined });
const d2 = Object.getOwnPropertyDescriptor(t2, "k") as any;
console.log("present_undefined=e=" + d2.enumerable + ",w=" + d2.writable);

function attempt(label: string, desc: any): void {
  try {
    Object.defineProperty({} as any, "k", desc);
    console.log(label + "=ok");
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

// get/set must be callable or undefined
attempt("get_number", { get: 1 });
attempt("get_null", { get: null });
attempt("get_undefined", { get: undefined });
attempt("set_object", { set: {} });
attempt("get_arrow", { get: () => 1 });
attempt("get_class", { get: class { } });

// a data field and an accessor field together is always a TypeError
attempt("value_and_get", { value: 1, get() { return 2; } });
attempt("value_and_set", { value: 1, set() { /* noop */ } });
attempt("writable_and_get", { writable: true, get() { return 2; } });
attempt("writable_and_set_undefined", { writable: true, set: undefined });
attempt("value_undefined_and_get", { value: undefined, get() { return 2; } });

// unknown fields on the descriptor are ignored
const t3: any = {};
Object.defineProperty(t3, "k", { value: 5, enumerable: true, bogus: "ignored", VALUE: 9 } as any);
const d3 = Object.getOwnPropertyDescriptor(t3, "k") as any;
console.log("extra_fields=" + Object.keys(d3).join("|") + ",v=" + d3.value);

// the descriptor must be an object
attempt("desc_number", 1 as any);
attempt("desc_string", "value" as any);
attempt("desc_null", null as any);
attempt("desc_undefined", undefined as any);

// a FUNCTION works as a descriptor object: its own properties are read
function fnDesc(): void { /* noop */ }
(fnDesc as any).value = "from-function";
(fnDesc as any).enumerable = true;
const t4: any = {};
Object.defineProperty(t4, "k", fnDesc as any);
console.log("fn_desc=" + t4.k + ",keys=" + Object.keys(t4).join("|"));

// the KEY is coerced BEFORE the descriptor is validated
const keyLog: string[] = [];
const weirdKey: any = { toString() { keyLog.push("key"); return "coerced"; } };
try {
  Object.defineProperty({} as any, weirdKey, { get: 1 } as any);
  console.log("key_vs_desc=ok");
} catch (e: any) {
  console.log("key_vs_desc=throw:" + e.constructor.name);
}
console.log("key_read=" + keyLog.join("|"));

// Object.defineProperties reads the map's own ENUMERABLE keys only
const map: any = { a: { value: 1, enumerable: true } };
Object.defineProperty(map, "b", { value: { value: 2, enumerable: true }, enumerable: false });
const t5: any = Object.defineProperties({}, map);
console.log("defineProperties=" + Reflect.ownKeys(t5).join("|"));
