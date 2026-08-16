// Cross-runtime: DOMException is a web-platform class wearing Error's clothes —
// it inherits from Error, but name/message/code are ACCESSORS on its prototype
// rather than own data properties, which is where an Error subclass would put them.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

// The three fields, and their defaults.
console.log("full=" + (function (): string { const d = new DOMException("boom", "AbortError"); return d.name + "|" + d.message + "|" + d.code; })());
console.log("message_only=" + (function (): string { const d = new DOMException("boom"); return d.name + "|" + d.message + "|" + d.code; })());
console.log("no_args=" + (function (): string { const d = new DOMException(); return d.name + "|" + JSON.stringify(d.message) + "|" + d.code; })());
console.log("undefined_args=" + (function (): string { const d = new DOMException(undefined, undefined); return d.name + "|" + JSON.stringify(d.message) + "|" + d.code; })());
console.log("empty_strings=" + (function (): string { const d = new DOMException("", ""); return JSON.stringify(d.name) + "|" + JSON.stringify(d.message) + "|" + d.code; })());
console.log("unknown_name=" + (function (): string { const d = new DOMException("m", "NotARealName"); return d.name + "|" + d.code; })());
console.log("message_coerced=" + (function (): string { const d = new DOMException(123 as any); return JSON.stringify(d.message) + "|" + typeof d.message; })());
console.log("message_null=" + (function (): string { const d = new DOMException(null as any); return JSON.stringify(d.message); })());
console.log("name_coerced=" + (function (): string { const d = new DOMException("m", 5 as any); return d.name + "|" + typeof d.name + "|" + d.code; })());
console.log("throwing_argument=" + t(function () { return new DOMException({ toString: function (): string { throw new RangeError("x"); } } as any); }));

// The prototype chain, and the accessors that live on it.
console.log("inherits_error=" + (new DOMException() instanceof Error) + "/" + (new DOMException() instanceof DOMException));
console.log("proto_chain=" + (Object.getPrototypeOf(DOMException.prototype) === Error.prototype) + "/" + (Object.getPrototypeOf(DOMException) === (Error as any)));
console.log("constructor_link=" + (DOMException.prototype.constructor === DOMException) + "/" + (new DOMException().constructor === DOMException));
console.log("accessors=" + ["name", "message", "code"].map(function (k) {
  const d: any = Object.getOwnPropertyDescriptor(DOMException.prototype, k);
  return k + ":" + (d ? (d.get ? "getter" : "data") : "absent");
}).join(","));
console.log("accessor_attrs=" + (function (): string {
  const d: any = Object.getOwnPropertyDescriptor(DOMException.prototype, "name");
  return "set:" + String(d.set) + " e:" + d.enumerable + " c:" + d.configurable;
})());
console.log("getter_on_plain=" + t(function () {
  const d: any = Object.getOwnPropertyDescriptor(DOMException.prototype, "name");
  return d.get.call({});
}));
console.log("no_own_name=" + Object.prototype.hasOwnProperty.call(new DOMException("m", "X"), "name") + "/" + Object.prototype.hasOwnProperty.call(new DOMException("m", "X"), "message"));
console.log("error_has_own=" + Object.prototype.hasOwnProperty.call(new Error("m"), "message") + "/" + Object.prototype.hasOwnProperty.call(new Error("m"), "name"));
console.log("tag=" + Object.prototype.toString.call(new DOMException()) + " tag_of_error=" + Object.prototype.toString.call(new Error()));
console.log("tostring=" + String(new DOMException("boom", "AbortError")) + " | " + String(new DOMException()) + " | " + String(new DOMException("only")));
console.log("tostring_is_error_s=" + (DOMException.prototype.toString === Error.prototype.toString));

// The constructor itself.
console.log("ctor_shape=" + typeof DOMException + " name=" + DOMException.name + " length=" + DOMException.length);
console.log("no_new=" + t(function () { return (DOMException as any)("m"); }));

// The legacy code constants sit on BOTH the constructor and the prototype.
console.log("constants_paired=" + ["INDEX_SIZE_ERR", "NOT_FOUND_ERR", "SYNTAX_ERR", "INVALID_STATE_ERR", "ABORT_ERR", "DATA_CLONE_ERR"].map(function (k) {
  return k + ":" + (DOMException as any)[k] + ((DOMException as any)[k] === (DOMException.prototype as any)[k] ? "=" : "!");
}).join(" "));
console.log("constant_attrs=" + (function (): string {
  const d: any = Object.getOwnPropertyDescriptor(DOMException, "ABORT_ERR");
  return "w:" + d.writable + " e:" + d.enumerable + " c:" + d.configurable;
})());

// A subclass keeps working, because the accessors read internal state set by the
// super() call rather than an own property.
console.log("subclass=" + t(function () {
  class MyAbort extends DOMException {
    constructor() {
      super("stopped", "AbortError");
    }
  }
  const e = new MyAbort();
  return e.name + "/" + e.message + "/" + e.code + "/" + (e instanceof DOMException) + "/" + (e instanceof Error) + "/" + e.constructor.name;
}));
console.log("subclass_tag=" + t(function () {
  class MyAbort extends DOMException {}
  return Object.prototype.toString.call(new MyAbort());
}));
console.log("catchable=" + (function (): string {
  try {
    throw new DOMException("thrown", "DataCloneError");
  } catch (e: any) {
    return e.name + "/" + e.code + "/" + (e instanceof DOMException);
  }
})());
console.log("structuredclone_uses_it=" + (function (): string {
  try {
    structuredClone(function () { return; });
    return "no-throw";
  } catch (e: any) {
    return (e instanceof DOMException) + "/" + e.name + "/" + e.code;
  }
})());
console.log("cloneable=" + t(function () {
  const c: any = structuredClone(new DOMException("m", "AbortError"));
  return c.name + "/" + c.message + "/" + (c instanceof DOMException);
}));
