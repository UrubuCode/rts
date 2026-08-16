// Cross-runtime: WHERE Symbol.toStringTag is actually written in the standard
// library. Most built-ins carry it as a non-writable, non-enumerable,
// CONFIGURABLE own data property of a prototype or a namespace object — and
// the ones that omit it are the ones whose tag comes from an internal slot.

function tagOf(label: string, o: any): void {
  if (o === undefined || o === null) { console.log(label + "=host_absent"); return; }
  const d: any = Object.getOwnPropertyDescriptor(o, Symbol.toStringTag);
  if (d === undefined) { console.log(label + "=absent:inherited=" + String((o as any)[Symbol.toStringTag])); return; }
  if (d.get !== undefined || d.set !== undefined) {
    console.log(label + "=accessor:get=" + typeof d.get + ",set=" + typeof d.set + "," + d.enumerable + "," + d.configurable);
    return;
  }
  console.log(label + "=data:" + JSON.stringify(d.value) + ":" + d.writable + "," + d.enumerable + "," + d.configurable);
}

// --- namespace objects hold it directly ---
tagOf("Math", Math);
tagOf("JSON", JSON);
tagOf("Reflect", Reflect);
tagOf("Atomics", Atomics);

// --- prototypes of the collection and box types ---
tagOf("Map.prototype", Map.prototype);
tagOf("Set.prototype", Set.prototype);
tagOf("WeakMap.prototype", WeakMap.prototype);
tagOf("WeakSet.prototype", WeakSet.prototype);
tagOf("Symbol.prototype", Symbol.prototype);
tagOf("BigInt.prototype", BigInt.prototype);
tagOf("Promise.prototype", Promise.prototype);
tagOf("ArrayBuffer.prototype", ArrayBuffer.prototype);
tagOf("SharedArrayBuffer.prototype", typeof SharedArrayBuffer === "undefined" ? undefined : SharedArrayBuffer.prototype);
tagOf("DataView.prototype", DataView.prototype);
tagOf("WeakRef.prototype", WeakRef.prototype);
tagOf("FinalizationRegistry.prototype", FinalizationRegistry.prototype);

// --- the ones that do NOT carry it: the tag comes from the internal slot ---
tagOf("Object.prototype", Object.prototype);
tagOf("Array.prototype", Array.prototype);
tagOf("Function.prototype", Function.prototype);
tagOf("String.prototype", String.prototype);
tagOf("Number.prototype", Number.prototype);
tagOf("Boolean.prototype", Boolean.prototype);
tagOf("Date.prototype", Date.prototype);
tagOf("RegExp.prototype", RegExp.prototype);
tagOf("Error.prototype", Error.prototype);

// --- iterator prototypes ---
tagOf("map_iterator", Object.getPrototypeOf(new Map().entries()));
tagOf("set_iterator", Object.getPrototypeOf(new Set().values()));
tagOf("array_iterator", Object.getPrototypeOf([].values()));
tagOf("string_iterator", Object.getPrototypeOf(""[Symbol.iterator]()));
tagOf("regexp_string_iterator", Object.getPrototypeOf("a".matchAll(/a/g)));
tagOf("iterator_prototype", Iterator.prototype);
tagOf("iterator_helper", Object.getPrototypeOf((new Set([1]).values() as any).map((x: any) => x)));

// --- generator machinery ---
function* g(): any { yield 1; }
const genObj = g();
tagOf("generator_object_proto_proto", Object.getPrototypeOf(Object.getPrototypeOf(genObj)));
tagOf("generator_function_proto", Object.getPrototypeOf(g));
async function* ag(): any { yield 1; }
tagOf("async_generator_function_proto", Object.getPrototypeOf(ag));
async function af(): Promise<void> { /* shape only */ }
tagOf("async_function_proto", Object.getPrototypeOf(af));

// --- typed arrays: the %TypedArray% prototype uses an ACCESSOR, the concrete
//     ones use a plain string ---
const TypedArrayProto = Object.getPrototypeOf(Uint8Array.prototype);
tagOf("typedarray_abstract", TypedArrayProto);
tagOf("uint8_prototype", Uint8Array.prototype);
tagOf("float64_prototype", Float64Array.prototype);
const tad: any = Object.getOwnPropertyDescriptor(TypedArrayProto, Symbol.toStringTag);
console.log("typedarray_getter_name=" + tad.get.name + ":len=" + tad.get.length);
console.log("typedarray_getter_on_u8=" + String(tad.get.call(new Uint8Array(1))));
console.log("typedarray_getter_on_plain=" + String(tad.get.call({})));
console.log("typedarray_getter_on_number=" + String(tad.get.call(5)));

// --- what each of those produces through Object.prototype.toString ---
function printed(label: string, v: any): void {
  console.log(label + "=" + Object.prototype.toString.call(v));
}
printed("math_obj", Math);
printed("json_obj", JSON);
printed("map_obj", new Map());
printed("promise_obj", Promise.resolve(1));
printed("symbol_prim", Symbol("x"));
printed("bigint_prim", 1n);
printed("dataview_obj", new DataView(new ArrayBuffer(1)));
printed("uint8_obj", new Uint8Array(1));
printed("generator_obj", genObj);
printed("generator_fn", g);
printed("async_fn", af);
printed("weakref_obj", new WeakRef({}));
printed("map_iter_obj", new Map().entries());
printed("arguments_obj", (function () { return arguments; })());

// --- a tag is inherited, so a subclass keeps the parent's until it declares
//     one of its own ---
class MySet extends Set<number> {}
printed("subclass_default", new MySet());
class TaggedSet extends Set<number> {
  get [Symbol.toStringTag]() { return "TaggedSet"; }
}
printed("subclass_tagged", new TaggedSet());
tagOf("TaggedSet.prototype", TaggedSet.prototype);

// --- every one of them is configurable, so a program may retag a built-in ---
const mathTag: any = Object.getOwnPropertyDescriptor(Math, Symbol.toStringTag);
console.log("retag_reflect_set=" + Reflect.set(Math, Symbol.toStringTag, "Nope"));
console.log("retag_unchanged=" + Object.prototype.toString.call(Math));
Object.defineProperty(Math, Symbol.toStringTag, { value: "Arithmetic", writable: false, enumerable: false, configurable: true });
console.log("retag_defined=" + Object.prototype.toString.call(Math));
Object.defineProperty(Math, Symbol.toStringTag, mathTag);
console.log("retag_restored=" + Object.prototype.toString.call(Math));
