// ONE thing: the IDENTITY graph between intrinsics. Several objects that are
// reachable by two different routes must be the SAME object, and several that
// look alike must be distinct. An engine that builds a fresh object per access
// answers every behaviour question correctly and fails this one.
function same(label: string, a: any, b: any) { console.log(label + "=" + (a === b)); }

// The iterator prototype chain: every built-in iterator shares %IteratorPrototype%.
const arrayIterProto = Object.getPrototypeOf(Object.getPrototypeOf([].values()));
const stringIterProto = Object.getPrototypeOf(Object.getPrototypeOf(""[Symbol.iterator]()));
const mapIterProto = Object.getPrototypeOf(Object.getPrototypeOf(new Map().values()));
const setIterProto = Object.getPrototypeOf(Object.getPrototypeOf(new Set().values()));
same("iterProto_array_string", arrayIterProto, stringIterProto);
same("iterProto_array_map", arrayIterProto, mapIterProto);
same("iterProto_array_set", arrayIterProto, setIterProto);
console.log("iterProtoHasSelfIterator=" + (typeof (arrayIterProto as any)[Symbol.iterator] === "function"));
console.log("iterProtoParentIsObject=" + (Object.getPrototypeOf(arrayIterProto) === Object.prototype));

// Map's values and keys iterators share a prototype; entries too.
same("mapValuesKeys", Object.getPrototypeOf(new Map().values()), Object.getPrototypeOf(new Map().keys()));
same("setValuesKeys", Object.getPrototypeOf(new Set().values()), Object.getPrototypeOf(new Set().keys()));

// Set.prototype.keys IS values; Map.prototype.entries IS Symbol.iterator.
same("setKeysIsValues", Set.prototype.keys, Set.prototype.values);
same("setIteratorIsValues", (Set.prototype as any)[Symbol.iterator], Set.prototype.values);
same("mapIteratorIsEntries", (Map.prototype as any)[Symbol.iterator], Map.prototype.entries);
same("arrayIteratorIsValues", (Array.prototype as any)[Symbol.iterator], Array.prototype.values);

// %TypedArray% is the shared prototype of every concrete typed array.
const taProto = Object.getPrototypeOf(Uint8Array.prototype);
same("int8SharesTA", Object.getPrototypeOf(Int8Array.prototype), taProto);
same("float64SharesTA", Object.getPrototypeOf(Float64Array.prototype), taProto);
same("bigint64SharesTA", Object.getPrototypeOf(BigInt64Array.prototype), taProto);
const taCtor = Object.getPrototypeOf(Uint8Array);
same("ctorsShareTA", Object.getPrototypeOf(Float32Array), taCtor);
console.log("taCtorNotFunction=" + (taCtor !== Function.prototype));
try { new (taCtor as any)(1); } catch (e: any) { console.log("taCtorNotConstructable=" + e.constructor.name); }

// Generator/async intrinsics.
function* g() {}
async function* ag() {}
async function af() {}
const genFnProto = Object.getPrototypeOf(g);
same("genFnProtoShared", Object.getPrototypeOf(function* () {}), genFnProto);
console.log("genFnProtoIsFunction=" + (Object.getPrototypeOf(genFnProto) === Function.prototype));
same("genObjProtoFromFn", Object.getPrototypeOf(g()), g.prototype);
console.log("genProtoParentShared=" + (Object.getPrototypeOf(Object.getPrototypeOf(g())) === (genFnProto as any).prototype));
same("asyncFnProtoShared", Object.getPrototypeOf(af), Object.getPrototypeOf(async function () {}));
console.log("asyncGenDistinct=" + (Object.getPrototypeOf(ag) !== genFnProto));

// A class and a plain function differ in prototype-descriptor writability.
class C {}
const classProtoDesc: any = Object.getOwnPropertyDescriptor(C, "prototype");
const fnProtoDesc: any = Object.getOwnPropertyDescriptor(function () {}, "prototype");
console.log("classProto=" + classProtoDesc.writable + "," + classProtoDesc.enumerable + "," + classProtoDesc.configurable);
console.log("fnProto=" + fnProtoDesc.writable + "," + fnProtoDesc.enumerable + "," + fnProtoDesc.configurable);
console.log("arrowHasNoProto=" + (Object.getOwnPropertyDescriptor(() => {}, "prototype") === undefined));
console.log("methodHasNoProto=" + (Object.getOwnPropertyDescriptor({ m() {} }.m, "prototype") === undefined));
console.log("genFnHasProto=" + (Object.getOwnPropertyDescriptor(g, "prototype") !== undefined));

// Error prototypes chain through Error.prototype but the constructors chain
// through Error itself.
for (const E of [TypeError, RangeError, SyntaxError, ReferenceError, EvalError, URIError]) {
  console.log(E.name + "=" + (Object.getPrototypeOf(E.prototype) === Error.prototype) + "," + (Object.getPrototypeOf(E) === Error));
}
console.log("aggregateChain=" + (Object.getPrototypeOf(AggregateError.prototype) === Error.prototype));

// The two ways to reach Object.prototype and Function.prototype agree.
same("objProto", Object.getPrototypeOf({}), Object.prototype);
same("fnProtoRoot", Object.getPrototypeOf(Function.prototype), Object.prototype);
same("objCtorIsFunction", Object.getPrototypeOf(Object), Function.prototype);
console.log("fnProtoIsCallable=" + (typeof Function.prototype === "function") + " returns=" + String((Function.prototype as any)()));
