// Pins Reflect.apply/construct argument handling: the argument list goes
// through CreateListFromArrayLike (a plain array-like works, a string does not,
// a non-object throws), and newTarget decides the PROTOTYPE of the result while
// the target still runs the body. 193_reflect_construct covers the basic call.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

// thisArg is always an object here: what a SLOPPY callee does with a primitive
// or nullish `this` is a property of the callee's mode, and this file is loaded
// as a script by one host and as a module by another.
function sum(this: any, ...args: number[]): string {
  return "this=" + (this as any).tag + ",n=" + args.length + ",sum=" + args.reduce((a, b) => a + b, 0);
}
const T: any = { tag: "T" };

console.log("array=" + Reflect.apply(sum, T, [1, 2, 3]));
console.log("arraylike=" + Reflect.apply(sum, T, { length: 2, 0: 10, 1: 20 } as any));
console.log("holes=" + Reflect.apply(sum, T, { length: 3, 0: 1 } as any));
console.log("len_coerced=" + Reflect.apply(sum, T, { length: "2", 0: 5, 1: 6 } as any));
console.log("len_float=" + Reflect.apply(sum, T, { length: 2.9, 0: 5, 1: 6 } as any));
console.log("len_neg=" + Reflect.apply(sum, T, { length: -1, 0: 5 } as any));
console.log("len_missing=" + Reflect.apply(sum, T, {} as any));
console.log("set=" + Reflect.apply(sum, T, new Set([1, 2, 3]) as any));

// a STRING is not an acceptable argument list, nor is any other primitive
attempt("string_args", () => Reflect.apply(sum, null, "12" as any));
attempt("number_args", () => Reflect.apply(sum, null, 3 as any));
attempt("null_args", () => Reflect.apply(sum, null, null as any));
attempt("missing_args", () => (Reflect.apply as any)(sum, null));
attempt("nonfn", () => (Reflect.apply as any)({}, null, []));

// thisArg is passed verbatim: the very object, not a copy and not a box
const probe: any = { tag: "PROBE" };
let sawSame = false;
function identity(this: any): string { sawSame = this === probe; return "tag=" + this.tag; }
console.log("this_identity=" + Reflect.apply(identity, probe, []) + ",same=" + sawSame);

// CreateListFromArrayLike reads length once, then the indices in ascending order
const reads: string[] = [];
const argSource: any = {};
Object.defineProperty(argSource, "length", { get() { reads.push("length"); return 3; } });
Object.defineProperty(argSource, "2", { get() { reads.push("2"); return 30; } });
Object.defineProperty(argSource, "0", { get() { reads.push("0"); return 10; } });
Object.defineProperty(argSource, "1", { get() { reads.push("1"); return 20; } });
console.log("ordered=" + Reflect.apply(sum, T, argSource));
console.log("read_order=" + reads.join("|"));

// Reflect.construct: newTarget decides .prototype, target runs the body
class Base {
  constructor() {
    (this as any).from = "Base";
    (this as any).nt = new.target === undefined ? "none" : new.target.name;
  }
}
class Other { }
(Other as any).prototype.marker = "OTHER";

const viaBase: any = Reflect.construct(Base, []);
console.log("plain=" + viaBase.from + "," + viaBase.nt + ",proto=" + (Object.getPrototypeOf(viaBase) === Base.prototype));

const viaOther: any = Reflect.construct(Base, [], Other);
console.log("newtarget_from=" + viaOther.from + ",nt=" + viaOther.nt);
console.log("newtarget_proto=" + (Object.getPrototypeOf(viaOther) === Other.prototype));
console.log("newtarget_marker=" + viaOther.marker);
console.log("newtarget_instanceof=" + (viaOther instanceof Other) + "," + (viaOther instanceof Base));

// newTarget must be a constructor
attempt("nt_arrow", () => String(Reflect.construct(Base, [], (() => 1) as any)));
attempt("nt_plain_object", () => String(Reflect.construct(Base, [], {} as any)));
attempt("nt_method", () => String(Reflect.construct(Base, [], ({ m() { return 1; } }).m as any)));

// a BOUND function has no own .prototype, so newTarget falls back to Object.prototype
function Bindable(this: any): void { (this as any).b = 1; }
(Bindable as any).prototype.tagged = "BINDABLE";
const bound: any = Bindable.bind(null);
const viaBound: any = Reflect.construct(Base, [], bound);
console.log("bound_proto=" + viaBound.tagged + ",instanceof=" + (viaBound instanceof Bindable));

// a newTarget whose .prototype is not an object falls back to Object.prototype
function NoProto(this: any): void { /* noop */ }
(NoProto as any).prototype = 7;
const viaNoProto: any = Reflect.construct(Base, [], NoProto);
console.log("noproto=" + (Object.getPrototypeOf(viaNoProto) === Object.prototype));

// arrow functions and methods are not constructable at all
attempt("construct_arrow", () => String(Reflect.construct((() => 1) as any, [])));
attempt("construct_method", () => String(Reflect.construct(({ m() { return 1; } }).m as any, [])));
attempt("construct_getter", () => String(Reflect.construct((Object.getOwnPropertyDescriptor({ get g() { return 1; } }, "g") as any).get, [])));

// builtins reached through construct
console.log("array_ctor=" + JSON.stringify(Reflect.construct(Array, [1, 2, 3])));
console.log("array_len=" + (Reflect.construct(Array, [3]) as any[]).length);
console.log("map_ctor=" + (Reflect.construct(Map, [[["k", 1]]]) as Map<string, number>).get("k"));
console.log("object_ctor=" + (Reflect.construct(Object, [5]) as any).valueOf());
attempt("construct_symbol", () => String(Reflect.construct(Symbol as any, [])));
attempt("construct_parseint", () => String(Reflect.construct(parseInt as any, ["7"])));

// the argument list of construct is coerced the same way
console.log("construct_arraylike=" + JSON.stringify(Reflect.construct(Array, { length: 2, 0: "a", 1: "b" } as any)));
attempt("construct_string_args", () => String(Reflect.construct(Array, "ab" as any)));
