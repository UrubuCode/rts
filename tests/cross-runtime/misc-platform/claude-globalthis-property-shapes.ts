// ONE thing: the SHAPE of the global object's own properties. The value
// properties are non-writable non-enumerable non-configurable, the constructors
// are writable and configurable but never enumerable, and globalThis itself is
// writable-configurable — a detail an engine that installs globals as plain
// data properties gets wrong.
function shape(name: string) {
  const d: any = Object.getOwnPropertyDescriptor(globalThis, name);
  if (!d) { console.log(name + "=<absent>"); return; }
  if ("get" in d && (d.get || d.set)) {
    console.log(name + "=accessor get:" + typeof d.get + " set:" + typeof d.set + " e:" + d.enumerable + " c:" + d.configurable);
  } else {
    console.log(name + "=data w:" + d.writable + " e:" + d.enumerable + " c:" + d.configurable);
  }
}

// The three value properties are frozen by the spec.
shape("undefined");
shape("NaN");
shape("Infinity");

// globalThis is writable and configurable but not enumerable.
shape("globalThis");

// Function properties.
shape("parseInt");
shape("parseFloat");
shape("isNaN");
shape("isFinite");
shape("decodeURI");
shape("encodeURIComponent");

// Constructor properties.
shape("Object");
shape("Array");
shape("Function");
shape("Symbol");
shape("Promise");
shape("Proxy");
shape("Reflect");
shape("Math");
shape("JSON");

// None of the standard globals is enumerable, so a bare for-in over globalThis
// must not report any of them.
const enumerable = Object.keys(globalThis).filter((k) =>
  ["Object", "Array", "Math", "JSON", "undefined", "NaN", "Infinity", "globalThis", "Promise"].indexOf(k) >= 0);
console.log("enumerableStandard=" + enumerable.length);

// Identity: globalThis is its own property and the same object.
console.log("selfRef=" + ((globalThis as any).globalThis === globalThis));
console.log("typeofGlobal=" + typeof globalThis);
console.log("tag=" + (Object.prototype.toString.call(globalThis).indexOf("[object ") === 0));

// Math and JSON are ordinary objects with a toStringTag, not constructors.
console.log("mathTag=" + Object.prototype.toString.call(Math));
console.log("jsonTag=" + Object.prototype.toString.call(JSON));
console.log("reflectTag=" + Object.prototype.toString.call(Reflect));
console.log("mathIsCtor=" + (typeof (Math as any) === "function"));

// The value properties really are immutable, probed mode-independently.
console.log("setUndefined=" + Reflect.set(globalThis, "undefined", 1) + " still=" + String((globalThis as any).undefined));
console.log("setNaN=" + Reflect.set(globalThis, "NaN", 1) + " still=" + String((globalThis as any).NaN));
console.log("delUndefined=" + Reflect.deleteProperty(globalThis, "undefined"));

// A fresh global name is a normal writable, enumerable, configurable property.
(globalThis as any).__probe = 1;
shape("__probe");
console.log("delProbe=" + Reflect.deleteProperty(globalThis, "__probe") + " gone=" + !("__probe" in globalThis));
