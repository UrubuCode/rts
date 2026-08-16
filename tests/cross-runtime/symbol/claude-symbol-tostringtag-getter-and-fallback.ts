// Cross-runtime: how Object.prototype.toString READS Symbol.toStringTag — it is
// a plain Get, so an accessor runs (and can throw), and a non-string tag is
// ignored in favour of the built-in tag.

// --- a getter is invoked, once per toString call ---
let gets = 0;
const dyn: any = {};
Object.defineProperty(dyn, Symbol.toStringTag, {
  get() { gets++; return "Dyn" + gets; },
  configurable: true,
});
console.log("dyn1=" + Object.prototype.toString.call(dyn));
console.log("dyn2=" + Object.prototype.toString.call(dyn));
console.log("gets=" + gets);

// --- a getter that throws propagates out of toString ---
const boom: any = {};
Object.defineProperty(boom, Symbol.toStringTag, {
  get() { throw new RangeError("tag"); },
  configurable: true,
});
try { Object.prototype.toString.call(boom); console.log("throwing_tag=no_throw"); }
catch (e: any) { console.log("throwing_tag=" + e.constructor.name); }
console.log("throwing_still_object=" + (typeof boom));

// --- a non-string tag is ignored; the built-in tag wins ---
function tagged(v: any): string {
  const o: any = {};
  Object.defineProperty(o, Symbol.toStringTag, { value: v, configurable: true });
  return Object.prototype.toString.call(o);
}
console.log("tag_number=" + tagged(42));
console.log("tag_null=" + tagged(null));
console.log("tag_undefined=" + tagged(undefined));
console.log("tag_bool=" + tagged(true));
console.log("tag_object=" + tagged({}));
console.log("tag_symbol=" + tagged(Symbol("s")));
console.log("tag_empty_string=" + tagged(""));
console.log("tag_string=" + tagged("Ok"));

// --- the built-in tag depends on the internal slot, not the prototype ---
function builtin(v: any): string { return Object.prototype.toString.call(v); }
console.log("builtin_array=" + builtin([]));
console.log("builtin_function=" + builtin(function () { /* fn */ }));
console.log("builtin_arrow=" + builtin(() => 1));
console.log("builtin_error=" + builtin(new Error("x")));
console.log("builtin_typeerror=" + builtin(new TypeError("x")));
console.log("builtin_date=" + builtin(new Date(0)));
console.log("builtin_regexp=" + builtin(/x/));
console.log("builtin_boolean_box=" + builtin(new Boolean(true)));
console.log("builtin_number_box=" + builtin(new Number(1)));
console.log("builtin_string_box=" + builtin(new String("s")));
console.log("builtin_arguments=" + (function () { return builtin(arguments); })());
console.log("builtin_null=" + builtin(null));
console.log("builtin_undefined=" + builtin(undefined));
console.log("builtin_number=" + builtin(1));
console.log("builtin_string=" + builtin("s"));
console.log("builtin_nullproto=" + builtin(Object.create(null)));

// --- an array with a string tag: the tag wins for toString, Array.isArray does not care ---
const arr: any = [1, 2];
Object.defineProperty(arr, Symbol.toStringTag, { value: "NotArray", configurable: true });
console.log("array_tagged=" + Object.prototype.toString.call(arr));
console.log("array_isArray=" + Array.isArray(arr));
console.log("array_join=" + arr.join(","));
console.log("array_own_toString=" + arr.toString());

// --- the tag is INHERITED through the prototype chain ---
const proto: any = {};
Object.defineProperty(proto, Symbol.toStringTag, { value: "Inherited", configurable: true });
const child: any = Object.create(proto);
console.log("inherited_tag=" + Object.prototype.toString.call(child));
console.log("inherited_own=" + Object.getOwnPropertySymbols(child).length);

// --- a class may define it as a getter on the prototype ---
class Tagged {
  get [Symbol.toStringTag]() { return "TaggedClass"; }
}
console.log("class_tag=" + Object.prototype.toString.call(new Tagged()));
console.log("class_tag_enumerable=" + (Object.getOwnPropertyDescriptor(Tagged.prototype, Symbol.toStringTag) as any).enumerable);

// --- toString itself takes no arguments and is a plain method ---
console.log("toString_length=" + Object.prototype.toString.length);
console.log("toString_name=" + Object.prototype.toString.name);
console.log("toString_via_this=" + ({} as any).toString());
