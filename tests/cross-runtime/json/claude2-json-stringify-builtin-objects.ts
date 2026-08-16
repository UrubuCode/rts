// Cross-runtime: what JSON.stringify makes of the built-in object types. Only
// own ENUMERABLE string-keyed data is serialised, so every type whose contents
// live in an internal slot comes out empty — a Map, a Set and an ArrayBuffer
// are all "{}" — while a typed array's indices are real properties.

function out(label: string, v: any): void {
  console.log(label + "=" + String(JSON.stringify(v)));
}

// --- collections keep their data in a slot, so nothing is visible ---
out("map", new Map([["a", 1]]));
out("set", new Set([1, 2]));
out("weakmap", new WeakMap());
out("weakset", new WeakSet());
out("map_in_object", { m: new Map([["a", 1]]) });
out("set_in_array", [new Set([1])]);
out("map_with_own_prop", Object.assign(new Map([["a", 1]]), { tagged: true }));

// --- buffers likewise; a typed array's indices are own enumerable properties ---
out("arraybuffer", new ArrayBuffer(4));
out("dataview", new DataView(new ArrayBuffer(4)));
out("uint8", new Uint8Array([1, 2, 3]));
out("float64", new Float64Array([1.5, 2]));
out("int8_empty", new Int8Array(0));
out("uint8_nested", { data: new Uint8Array([7]) });
console.log("typedarray_is_not_array=" + Array.isArray(new Uint8Array([1])));

// --- the error family: message and stack are non-enumerable ---
out("error", new Error("boom"));
out("typeerror", new TypeError("boom"));
const withOwn: any = new Error("boom");
withOwn.code = "E_CODE";
out("error_with_own_prop", withOwn);
out("error_cause", new Error("boom", { cause: "why" }));
console.log("message_enumerable=" + Object.prototype.propertyIsEnumerable.call(new Error("x"), "message"));

// --- a regular expression has no own enumerable data either ---
out("regexp", /ab+c/gi);
out("regexp_with_prop", Object.assign(/x/, { note: "kept" }));

// --- dates have toJSON, so they are the exception ---
out("date", new Date(0));
out("invalid_date", new Date(NaN));

// --- boxed primitives are unwrapped by the algorithm ---
out("boxed_number", new Number(7));
out("boxed_string", new String("s"));
out("boxed_boolean", new Boolean(true));
out("boxed_in_array", [new Number(1), new String("a"), new Boolean(false)]);
const boxedWithProp: any = new Number(7);
boxedWithProp.extra = "ignored";
out("boxed_with_own_prop", boxedWithProp);

// --- functions and symbols vanish; a BigInt refuses outright ---
out("function", function f() { /* dropped */ });
out("arrow", () => 1);
out("class_ctor", class C {});
out("symbol", Symbol("s"));
try { JSON.stringify(1n); console.log("bigint=no_throw"); }
catch (e: any) { console.log("bigint=" + e.constructor.name); }
try { JSON.stringify({ v: 1n }); console.log("bigint_nested=no_throw"); }
catch (e: any) { console.log("bigint_nested=" + e.constructor.name); }

// --- promises and iterators are ordinary objects with nothing enumerable ---
out("promise", Promise.resolve(1));
out("map_iterator", new Map([["a", 1]]).entries());
out("generator_object", (function* () { yield 1; })());
out("weakref", new WeakRef({ a: 1 }));

// --- a class instance keeps its fields and loses its methods ---
class Point {
  x = 1;
  y = 2;
  get computed(): number { return 3; }
  sum(): number { return this.x + this.y; }
  static origin = "0,0";
}
out("class_instance", new Point());
console.log("accessor_on_proto_ignored=" + JSON.stringify(new Point()));
const withAccessor: any = { plain: 1 };
Object.defineProperty(withAccessor, "own_getter", { get() { return 9; }, enumerable: true });
Object.defineProperty(withAccessor, "hidden_getter", { get() { return 9; }, enumerable: false });
out("own_getters", withAccessor);

// --- a private field is invisible; a #private class still serialises fields ---
class Secret {
  #hidden = "no";
  shown = "yes";
  reveal(): string { return this.#hidden; }
}
out("private_field", new Secret());

// --- a null-prototype object works; inherited enumerables do not ---
const bare: any = Object.create(null);
bare.a = 1;
out("null_proto", bare);
const inherited: any = Object.create({ fromProto: 1 });
inherited.own = 2;
out("inherited_props", inherited);

// --- the arguments object is an ordinary object with index keys ---
out("arguments", (function () { return arguments; })(1, "two"));

// --- an array-like plain object is NOT an array ---
out("array_like", { 0: "a", 1: "b", length: 2 });
out("sparse_array", [1, , 3]);
out("array_with_string_prop", Object.assign([1, 2], { note: "dropped" }));
console.log("array_extra_prop_dropped=" + JSON.stringify(Object.assign([1], { extra: 1 })));

// --- a Map is empty even with entries; converting it first is the fix ---
const m = new Map<string, number>([["a", 1], ["b", 2]]);
out("map_raw", m);
out("map_via_fromEntries", Object.fromEntries(m));
out("map_via_entries_array", [...m]);
out("set_via_spread", [...new Set([1, 2])]);

// --- and a toJSON on the prototype rescues the type globally ---
(Map.prototype as any).toJSON = function (this: any) { return [...this]; };
out("map_with_toJSON", m);
out("map_nested_with_toJSON", { inner: m });
delete (Map.prototype as any).toJSON;
out("map_after_cleanup", m);
