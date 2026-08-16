// Cross-runtime: `this` is passed through UNCHANGED into a class method — class
// bodies are strict, so `null` stays undefined-ish, a primitive is not boxed
// into a wrapper object, and `apply` accepts any array-like as the arguments.

class Receiver {
  static describe(...tail: any[]): string {
    const self: any = this;
    return typeof self + ":" + String(self) + (tail.length ? "|args=" + tail.join(",") : "");
  }
  method(...tail: any[]): string {
    const self: any = this;
    return typeof self + ":" + String(self) + (tail.length ? "|args=" + tail.join(",") : "");
  }
}

const proto: any = Receiver.prototype;

// 1) A primitive receiver stays a primitive.
console.log("call_number=" + proto.method.call(7));
console.log("call_string=" + proto.method.call("s"));
console.log("call_boolean=" + proto.method.call(true));
console.log("call_symbol=" + proto.method.call(Symbol("k")).slice(0, 6));

// 2) `null` and `undefined` are not replaced by a global object.
console.log("call_null=" + proto.method.call(null));
console.log("call_undefined=" + proto.method.call(undefined));
console.log("call_nothing=" + proto.method.call());

// 3) The receiver is not boxed: it is `===` the value that was passed.
class Identity {
  same(v: any): boolean {
    return (this as any) === v;
  }
}
const idProto: any = Identity.prototype;
console.log("primitive_not_boxed=" + idProto.same.call(7, 7));
console.log("string_not_boxed=" + idProto.same.call("s", "s"));
console.log("object_identity=" + idProto.same.call(proto, proto));

// 4) An object receiver arrives as itself, and extra arguments follow.
const bag: any = { toString(): string { return "[bag]"; } };
console.log("call_object=" + proto.method.call(bag, 1, 2));
console.log("apply_object=" + proto.method.apply(bag, [3, 4]));

// 5) `apply` with a null or undefined argument list means no arguments.
console.log("apply_null_args=" + proto.method.apply(bag, null));
console.log("apply_undefined_args=" + proto.method.apply(bag, undefined));
console.log("apply_no_second=" + proto.method.apply(bag));

// 6) `apply` accepts any array-like: an object with a numeric `length`.
console.log("apply_arraylike=" + proto.method.apply(bag, { length: 3, 0: "a", 1: "b", 2: "c" }));

// 7) A missing index inside the array-like arrives as undefined.
console.log("apply_sparse=" + proto.method.apply(bag, { length: 3, 0: "a", 2: "c" }));

// 8) A `length` that is a GETTER is read once and decides the count.
let lengthReads = 0;
const withGetter: any = {
  0: "x",
  1: "y",
  2: "z",
  get length(): number {
    lengthReads += 1;
    return 2;
  },
};
console.log("apply_length_getter=" + proto.method.apply(bag, withGetter));
console.log("length_reads=" + lengthReads);

// 9) A `length` that is a string is coerced.
console.log("apply_length_string=" + proto.method.apply(bag, { length: "2", 0: "p", 1: "q" }));

// 10) A negative or fractional `length` is clamped and truncated.
console.log("apply_length_negative=" + proto.method.apply(bag, { length: -1, 0: "p" }));
console.log("apply_length_fraction=" + proto.method.apply(bag, { length: 1.9, 0: "p", 1: "q" }));

// 11) An object with no `length` at all passes no arguments.
console.log("apply_no_length=" + proto.method.apply(bag, { 0: "p" }));

// 12) The argument list must be an OBJECT: a boxed string is array-like and
//     spreads into characters, while the primitive it wraps is refused.
function applyWith(args: any): string {
  try {
    return "ok:" + proto.method.apply(bag, args);
  } catch (e) {
    return "threw:" + (e as any).constructor.name;
  }
}
console.log("apply_boxed_string=" + applyWith(Object("abc")));
console.log("apply_primitive_string=" + applyWith("abc"));

// 13) Other primitives are refused the same way.
console.log("apply_number=" + applyWith(7));
console.log("apply_boolean=" + applyWith(true));

// 14) A Set is NOT array-like, so it contributes nothing.
console.log("apply_set=" + proto.method.apply(bag, new Set([1, 2]) as any));

// 15) A real array and an `arguments` object behave the same.
function passesOwnArguments(a: number = 0): string {
  return proto.method.apply(bag, arguments);
}
console.log("apply_arguments=" + passesOwnArguments(1, 2 as any));

// 16) A static method behaves identically, with the class itself as the usual
//     receiver.
console.log("static_default=" + Receiver.describe().slice(0, 8));
console.log("static_call_null=" + Receiver.describe.call(null));

// 17) `call` with a receiver but no arguments, contrasted with `apply` with an
//     empty array.
console.log("call_empty=" + proto.method.call(bag));
console.log("apply_empty=" + proto.method.apply(bag, []));

// 18) Calling with a receiver that is a class instance reaches its fields.
class Named {
  label = "instance";
  show(): string {
    return "label=" + (this as any).label;
  }
}
const namedProto: any = Named.prototype;
console.log("instance_receiver=" + namedProto.show.call(new Named()));
console.log("foreign_receiver=" + namedProto.show.call({ label: "foreign" }));
console.log("undefined_receiver=" + (function (): string {
  try {
    return namedProto.show.call(undefined);
  } catch (e) {
    return "threw:" + (e as any).constructor.name;
  }
})());

// 19) `call` and `apply` are the same function object on every function.
console.log("call_shared=" + (proto.method.call === Function.prototype.call));
console.log("apply_shared=" + (proto.method.apply === Function.prototype.apply));
console.log("call_meta=" + JSON.stringify(Function.prototype.call.name) + "/" +
  Function.prototype.call.length + " apply=" + Function.prototype.apply.length);
