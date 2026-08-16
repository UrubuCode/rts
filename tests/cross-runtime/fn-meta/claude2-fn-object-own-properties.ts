// Cross-runtime: a function is an object. It carries user properties like any
// other, its `prototype` is a real object with a back-link, each evaluation of
// a function EXPRESSION makes a new object, and the built-in slots stay out of
// the enumerable listing.

function target(a: number, b: number): number { return a + b; }

// 1) Assigned properties are ordinary and enumerable; the built-in ones are not.
(target as any).cacheHits = 0;
(target as any).describe = "a function with baggage";
console.log("enumerable_keys=" + Object.keys(target).join(","));
console.log("json=" + JSON.stringify(target as any));
console.log("has_length_key=" + (Object.keys(target).indexOf("length") >= 0));
console.log("in_operator=" + ("length" in target) + "/" + ("cacheHits" in target));

// 2) The properties survive calls and can be mutated from inside.
function counted(): number {
  (counted as any).calls += 1;
  return (counted as any).calls;
}
(counted as any).calls = 0;
console.log("self_counter=" + counted() + "," + counted() + "," + counted());
console.log("counter_readable=" + (counted as any).calls);

// 3) A function's `prototype` is a plain object with a `constructor` back-link,
//    and that link is not enumerable.
console.log("prototype_type=" + typeof target.prototype);
console.log("prototype_backlink=" + (target.prototype.constructor === target));
const ctorDesc: any = Object.getOwnPropertyDescriptor(target.prototype, "constructor");
console.log("backlink_attrs=w=" + ctorDesc.writable + " e=" + ctorDesc.enumerable +
  " c=" + ctorDesc.configurable);
console.log("prototype_keys=" + JSON.stringify(Object.keys(target.prototype)));

// 4) `prototype` is writable on a declaration but not on a class — probed
//    rather than assigned, since a refused write throws only in strict code.
class Klass {}
const replacement = { marker: "new-proto" };
console.log("set_fn_prototype=" + Reflect.set(target, "prototype", replacement));
console.log("fn_prototype_now=" + ((target.prototype as any).marker === "new-proto"));
console.log("set_class_prototype=" + Reflect.set(Klass, "prototype", replacement));
console.log("class_prototype_unchanged=" + (Klass.prototype.constructor === Klass));

// 5) It is not configurable either, so it cannot be deleted.
console.log("delete_fn_prototype=" + Reflect.deleteProperty(target, "prototype"));
console.log("still_has_prototype=" + Object.prototype.hasOwnProperty.call(target, "prototype"));
console.log("delete_user_property=" + Reflect.deleteProperty(target, "describe"));
console.log("user_property_gone=" + Object.prototype.hasOwnProperty.call(target, "describe"));

// 6) Every evaluation of a function EXPRESSION makes a new object, with its own
//    `prototype` object.
function factory(): any {
  return function inner(): string { return "inner"; };
}
const one = factory();
const two = factory();
console.log("distinct_functions=" + (one !== two));
console.log("distinct_prototypes=" + (one.prototype !== two.prototype));
console.log("same_name=" + (one.name === two.name) + "/" + JSON.stringify(one.name));
(one as any).stamp = "first";
console.log("properties_not_shared=" + String((two as any).stamp));

// 7) The same is true of arrows and of closures over the same variable.
function arrowFactory(): any {
  return (): string => "arrow";
}
console.log("distinct_arrows=" + (arrowFactory() !== arrowFactory()));

// 8) A DECLARATION is hoisted once, so the same object is seen everywhere.
function stable(): string { return "stable"; }
const firstRef = stable;
const secondRef = stable;
console.log("declaration_identity=" + (firstRef === secondRef));

// 9) A function is an instance of Function and of Object.
console.log("instanceof=" + (target instanceof Function) + "/" + (target instanceof Object));
console.log("proto_chain=" + (Object.getPrototypeOf(target) === Function.prototype) + "/" +
  (Object.getPrototypeOf(Function.prototype) === Object.prototype));

// 10) It can be frozen, and then user properties stop changing while calls keep
//     working.
function frozenFn(): string { return "still-callable"; }
(frozenFn as any).mutable = "before";
Object.freeze(frozenFn);
console.log("frozen_set=" + Reflect.set(frozenFn, "mutable", "after") + "|value=" +
  (frozenFn as any).mutable);
console.log("frozen_call=" + frozenFn());
console.log("is_frozen=" + Object.isFrozen(frozenFn));

// 11) A function can be given accessors like any object.
function withAccessor(): string { return "base"; }
let stored = "x";
Object.defineProperty(withAccessor, "slot", {
  get(): string { return "got:" + stored; },
  set(v: string) { stored = v.toUpperCase(); },
  enumerable: true,
  configurable: true,
});
(withAccessor as any).slot = "written";
console.log("accessor_on_function=" + (withAccessor as any).slot);
console.log("accessor_enumerable=" + Object.keys(withAccessor).join(","));

// 12) A function used as a key in a Map or a Set works by identity.
const registry = new Map<any, string>();
registry.set(one, "one");
registry.set(two, "two");
console.log("map_by_identity=" + registry.get(one) + "," + registry.get(two) + "," +
  String(registry.get(factory())));

// 13) A function's own properties are copied by spread, but the function-ness
//     is not.
const spreadCopy: any = { ...(target as any) };
console.log("spread_copy_keys=" + Object.keys(spreadCopy).join(","));
console.log("spread_copy_type=" + typeof spreadCopy);

// 14) `Object.assign` onto a function keeps it callable.
const enriched: any = Object.assign(function base(): string { return "base"; }, { extra: 1 });
console.log("assign_onto_function=" + typeof enriched + "/" + enriched() + "/" + enriched.extra);

// 15) A function's `prototype` object can be replaced wholesale, which changes
//     what `instanceof` answers for instances made afterwards.
function Widget(): void {}
const madeBefore: any = new (Widget as any)();
Widget.prototype = { kind: "replaced" };
const madeAfter: any = new (Widget as any)();
console.log("before_instanceof=" + (madeBefore instanceof Widget));
console.log("after_instanceof=" + (madeAfter instanceof Widget));
console.log("after_kind=" + madeAfter.kind);

// 16) Two functions with identical source are still different objects.
const a1 = function (): number { return 1; };
const a2 = function (): number { return 1; };
console.log("identical_source_differs=" + (a1 !== a2) + "|results_equal=" + (a1() === a2()));

// 17) A function stored in an array or a Set keeps its identity.
const set = new Set([target, one, one]);
console.log("set_size=" + set.size + "|has_target=" + set.has(target));
