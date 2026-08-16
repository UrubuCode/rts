// Cross-runtime: how a user Error subclass resolves `name`, what
// Object.setPrototypeOf in the constructor fixes, and that captureStackTrace is
// V8-only surface whose presence is checked without printing anything.
class Plain extends Error {}
const p = new Plain("m");
console.log("plain-name=" + p.name);
console.log("plain-own-name=" + Object.prototype.hasOwnProperty.call(p, "name"));
console.log("plain-message=" + p.message);
console.log("plain-tostring=" + p.toString());
console.log("plain-instanceof=" + (p instanceof Plain) + ":" + (p instanceof Error));
console.log("plain-ctor=" + p.constructor.name);
console.log("plain-proto-ctor=" + (Plain.prototype.constructor === Plain));

// A `name` on the subclass PROTOTYPE is the idiomatic fix and stays non-own.
class Named extends Error {
  constructor(message: string) {
    super(message);
  }
}
Named.prototype.name = "Named";
const n = new Named("m");
console.log("named-name=" + n.name);
console.log("named-own-name=" + Object.prototype.hasOwnProperty.call(n, "name"));
console.log("named-tostring=" + n.toString());
console.log("named-keys=" + Object.keys(n).join(","));
console.log("named-json=" + JSON.stringify(n));

// Assigning this.name in the constructor makes it an ENUMERABLE own property,
// which JSON.stringify then picks up.
class OwnName extends Error {
  constructor(message: string) {
    super(message);
    this.name = "OwnName";
  }
}
const on = new OwnName("m");
console.log("own-name=" + on.name);
console.log("own-is-own=" + Object.prototype.hasOwnProperty.call(on, "name"));
const ond: any = Object.getOwnPropertyDescriptor(on, "name");
console.log("own-desc=w" + ond.writable + ",e" + ond.enumerable + ",c" + ond.configurable);
console.log("own-keys=" + Object.keys(on).join(","));
console.log("own-json=" + JSON.stringify(on));

// A class field named `name` behaves the same, defined rather than assigned.
class FieldName extends Error {
  name: string = "FieldName";
}
const fn = new FieldName("m");
console.log("field-name=" + fn.name);
console.log("field-keys=" + Object.keys(fn).join(","));

// setPrototypeOf in the constructor repairs a chain broken by a manual
// prototype swap; without it, instanceof fails.
class Broken extends Error {}
Object.setPrototypeOf(Broken.prototype, Object.prototype);
const br: any = new Broken("m");
console.log("broken-instanceof-error=" + (br instanceof Error));
console.log("broken-instanceof-self=" + (br instanceof Broken));
console.log("broken-name=" + String(br.name));
console.log("broken-tostring-tag=" + Object.prototype.toString.call(br));

class Repaired extends Error {
  constructor(message: string) {
    super(message);
    Object.setPrototypeOf(this, Repaired.prototype);
  }
}
const rp = new Repaired("m");
console.log("repaired-instanceof=" + (rp instanceof Repaired) + ":" + (rp instanceof Error));
console.log("repaired-proto=" + (Object.getPrototypeOf(rp) === Repaired.prototype));

// A subclass whose constructor returns a DIFFERENT error entirely.
class Swapper extends Error {
  constructor() {
    super("ignored");
    return new TypeError("swapped");
  }
}
const sw: any = new Swapper();
console.log("swap-ctor=" + sw.constructor.name);
console.log("swap-instanceof-swapper=" + (sw instanceof Swapper));
console.log("swap-instanceof-type=" + (sw instanceof TypeError));
console.log("swap-message=" + sw.message);

// stack: presence and type only, never the text.
const s: any = new Error("m");
console.log("stack-type=" + typeof s.stack);
console.log("stack-nonempty=" + (typeof s.stack === "string" ? s.stack.length > 0 : false));
console.log("stack-own=" + Object.prototype.hasOwnProperty.call(s, "stack"));
console.log("stack-enumerable=" + (Object.keys(s).indexOf("stack") >= 0));
console.log("subclass-stack-type=" + typeof (new Plain("m") as any).stack);

// captureStackTrace is optional; report the type, do not call-and-print.
console.log("capture-type=" + typeof (Error as any).captureStackTrace);
if (typeof (Error as any).captureStackTrace === "function") {
  const target: any = {};
  (Error as any).captureStackTrace(target);
  console.log("captured-type=" + typeof target.stack);
} else {
  console.log("captured-type=absent");
}

// A deeper hierarchy: three levels, each with its own prototype name.
class L1 extends Error {}
L1.prototype.name = "L1";
class L2 extends L1 {}
L2.prototype.name = "L2";
class L3 extends L2 {}
const l3 = new L3("deep");
console.log("l3-name=" + l3.name);
console.log("l3-chain=" + (l3 instanceof L1) + ":" + (l3 instanceof L2) + ":" + (l3 instanceof Error));
console.log("l3-tostring=" + l3.toString());
console.log("l3-ctor-chain=" + (Object.getPrototypeOf(L3) === L2));
