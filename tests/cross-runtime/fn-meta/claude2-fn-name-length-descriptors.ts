// Cross-runtime: `name` and `length` are real own properties with attributes —
// not writable, not enumerable, but CONFIGURABLE — so they can be redefined or
// deleted, and after deleting `name` the value falls through to
// Function.prototype.name.

function two(a: number, b: number): number { return a + b; }

function attrs(fn: any, key: string): string {
  const d: any = Object.getOwnPropertyDescriptor(fn, key);
  if (!d) return "absent";
  return "w=" + d.writable + " e=" + d.enumerable + " c=" + d.configurable + " v=" + JSON.stringify(d.value);
}

// 1) A plain declaration.
console.log("decl_name=" + attrs(two, "name"));
console.log("decl_length=" + attrs(two, "length"));

// 2) An arrow, a method and a class agree on the attributes.
const arrow = (a: number): number => a;
console.log("arrow_name=" + attrs(arrow, "name"));
console.log("arrow_length=" + attrs(arrow, "length"));

const holder = { m(a: number, b: number, c: number): number { return a + b + c; } };
console.log("method_name=" + attrs(holder.m, "name"));
console.log("method_length=" + attrs(holder.m, "length"));

class Klass { constructor(a: number, b: number) { void a; void b; } }
console.log("class_name=" + attrs(Klass, "name"));
console.log("class_length=" + attrs(Klass, "length"));

// 3) A bound function's pair is configurable too.
const bound = two.bind(null, 1);
console.log("bound_name=" + attrs(bound, "name"));
console.log("bound_length=" + attrs(bound, "length"));

// 4) Neither is enumerable, so they never show up in a key listing.
// (The full own-name listing is not compared: a sloppy-mode function carries
// legacy `caller`/`arguments` slots in some hosts. The trio is what is pinned.)
console.log("own_trio=" + ["length", "name", "prototype"]
  .map((k) => k + ":" + Object.prototype.hasOwnProperty.call(two, k)).join(","));
console.log("enumerable_keys=" + JSON.stringify(Object.keys(two)));

// 5) A write is refused rather than obeyed — probed, not thrown.
console.log("set_name_accepted=" + Reflect.set(two, "name", "renamed"));
console.log("name_after_set=" + two.name);
console.log("set_length_accepted=" + Reflect.set(two, "length", 9));
console.log("length_after_set=" + two.length);

// 6) Being configurable, both can be redefined.
function redefinable(a: number): number { return a; }
Object.defineProperty(redefinable, "name", { value: "given-a-new-name" });
Object.defineProperty(redefinable, "length", { value: 7 });
console.log("redefined_name=" + redefinable.name);
console.log("redefined_length=" + redefinable.length);
console.log("redefined_still_configurable=" +
  (Object.getOwnPropertyDescriptor(redefinable, "name") as any).configurable);

// 7) Deleting `name` leaves the inherited empty string behind.
function deletable(): void {}
console.log("before_delete=" + JSON.stringify(deletable.name));
console.log("delete_accepted=" + Reflect.deleteProperty(deletable, "name"));
console.log("has_own_name=" + Object.prototype.hasOwnProperty.call(deletable, "name"));
console.log("after_delete=" + JSON.stringify(deletable.name));
console.log("proto_name=" + JSON.stringify(Function.prototype.name));

// 8) Deleting `length` behaves the same way.
console.log("delete_length=" + Reflect.deleteProperty(deletable, "length"));
console.log("length_after_delete=" + deletable.length + "|proto=" + Function.prototype.length);

// 9) An anonymous function in a binding gets its name from the binding, and it
//    is still a configurable own property.
const inferred = function (): void {};
console.log("inferred_name=" + attrs(inferred, "name"));

// 10) An explicit name on a function expression wins over the binding.
const explicit = function realName(): void {};
console.log("explicit_name=" + explicit.name);

// 11) `length` stops counting at the first default and never counts the rest.
function defaults(a: number, b: number = 1, c: number = 2): number { return a + b + c; }
function rest(a: number, ...others: number[]): number { return a + others.length; }
function restOnly(...all: number[]): number { return all.length; }
console.log("defaults_length=" + defaults.length);
console.log("rest_length=" + rest.length);
console.log("rest_only_length=" + restOnly.length);

// 12) A destructuring parameter counts as one.
function patterns({ a }: any, [b]: any, c: number): number { return a + b + c; }
console.log("pattern_length=" + patterns.length);

// 13) Accessors: a getter has length 0, a setter 1, and their names carry the
//     prefix.
const accessors: any = {
  get thing(): number { return 1; },
  set thing(v: number) { void v; },
};
const d: any = Object.getOwnPropertyDescriptor(accessors, "thing");
console.log("getter=" + JSON.stringify(d.get.name) + "/" + d.get.length);
console.log("setter=" + JSON.stringify(d.set.name) + "/" + d.set.length);

// 14) A class's `prototype` is the one non-configurable, non-writable member of
//     the trio, unlike a declaration's.
const classProto: any = Object.getOwnPropertyDescriptor(Klass, "prototype");
const declProto: any = Object.getOwnPropertyDescriptor(two, "prototype");
console.log("class_prototype=w=" + classProto.writable + " e=" + classProto.enumerable +
  " c=" + classProto.configurable);
console.log("decl_prototype=w=" + declProto.writable + " e=" + declProto.enumerable +
  " c=" + declProto.configurable);

// 15) A bound function has no own `prototype` at all.
console.log("bound_prototype=" + attrs(bound, "prototype"));

// 16) Redefining `name` to a non-string is allowed by defineProperty.
function odd(): void {}
Object.defineProperty(odd, "name", { value: 123 });
console.log("odd_name=" + odd.name + "|type=" + typeof odd.name);

// 17) Making `name` writable afterwards lets a plain assignment through.
function loosened(): void {}
Object.defineProperty(loosened, "name", { value: "start", writable: true, configurable: true });
console.log("loosened_set=" + Reflect.set(loosened, "name", "changed") + "|" + loosened.name);

// 18) The pair survives on a function stored in an object and read back.
const stored: any = { fn: two };
console.log("stored_name=" + stored.fn.name + "|length=" + stored.fn.length);
