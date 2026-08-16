// Cross-runtime: how JSON.stringify DECIDES what its second and third
// arguments are. A replacer is a function if it is callable and a property list
// only if IsArray says so — array-LIKE is not enough — and a gap is unwrapped
// only from a genuine Number/String box, which an internal slot defines and a
// Proxy therefore does not have.

const value: any = { a: 1, b: { a: 2, c: 3 }, d: [4, 5] };

function s(replacer?: any, space?: any): string {
  try { return String((JSON.stringify as any)(value, replacer, space)); }
  catch (e: any) { return e.constructor.name; }
}

// --- a callable is a replacer function, whatever else it is ---
console.log("plain_function=" + s(function (k: string, v: any) { return typeof v === "number" ? v * 10 : v; }));
console.log("arrow=" + s((k: string, v: any) => (k === "c" ? undefined : v)));
console.log("bound=" + s((function (k: string, v: any) { return v; }).bind(null)));
console.log("class_ctor_as_replacer=" + s(class Nope { }));

// --- a callable Proxy of a function is still callable ---
const callableProxy: any = new Proxy(function (k: string, v: any) { return k === "a" ? "PROXIED" : v; }, {});
console.log("callable_proxy=" + s(callableProxy));
console.log("proxy_typeof=" + typeof callableProxy);

// --- IsArray pierces a Proxy, so a proxy of an array IS a property list ---
const arrayProxy: any = new Proxy(["a"], {});
console.log("array_proxy=" + s(arrayProxy));
console.log("isArray_proxy=" + Array.isArray(arrayProxy));

// --- but a revoked proxy of an array cannot be asked ---
const revoked = Proxy.revocable(["a"], {});
revoked.revoke();
console.log("revoked_array_proxy=" + s(revoked.proxy));
console.log("isArray_revoked=" + (function () { try { return String(Array.isArray(revoked.proxy)); } catch (e: any) { return e.constructor.name; } })());

// --- array-LIKE is ignored entirely ---
console.log("array_like=" + s({ 0: "a", length: 1 }));
console.log("arguments=" + s((function () { return arguments; })("a")));
console.log("typed_array=" + s(new Uint8Array([1])));
console.log("string_replacer=" + s("a"));
console.log("set_of_keys=" + s(new Set(["a"])));
console.log("array_proto_object=" + s(Object.create(Array.prototype)));
console.log("number_replacer=" + s(42));
console.log("null_replacer=" + s(null));
console.log("undefined_replacer=" + s(undefined));
console.log("boolean_replacer=" + s(true));
console.log("symbol_replacer=" + s(Symbol("s")));

// --- an Array SUBCLASS counts ---
class KeyList extends Array<string> { }
const keyList = KeyList.from(["a"]) as any;
console.log("array_subclass=" + s(keyList));
console.log("isArray_subclass=" + Array.isArray(keyList));

// --- the list is read as an ordinary indexed walk, holes included ---
const holed: any = ["a", , "c"];
console.log("holed_list=" + s(holed));
const withInherited: any = ["a"];
Object.setPrototypeOf(withInherited, { 1: "b" });
console.log("inherited_entry=" + s(withInherited));

// --- entries: strings and numbers count, wrappers are unwrapped, the rest go ---
function list(entries: any[]): string {
  return String(JSON.stringify({ a: 1, b: 2, 3: "three" }, entries as any));
}
console.log("entry_string=" + list(["a"]));
console.log("entry_number=" + list([3]));
console.log("entry_number_wrapper=" + list([new Number(3)]));
console.log("entry_string_wrapper=" + list([new String("a")]));
console.log("entry_boolean=" + list([true, "a"]));
console.log("entry_null=" + list([null, "a"]));
console.log("entry_undefined=" + list([undefined, "a"]));
console.log("entry_symbol=" + list([Symbol("a"), "a"]));
console.log("entry_object=" + list([{ toString() { return "a"; } }, "b"]));
console.log("entry_bigint=" + (function () { try { return list([1n, "a"]); } catch (e: any) { return e.constructor.name; } })());
console.log("entry_duplicate=" + list(["a", "a", "b"]));
console.log("entry_empty=" + list([]));

// --- a boxed gap is unwrapped; a proxy of one is not ---
const compact = String(JSON.stringify({ a: 1 }));
function gap(g: any): string {
  try { return JSON.stringify(String((JSON.stringify as any)({ a: 1 }, null, g))); }
  catch (e: any) { return e.constructor.name; }
}
console.log("gap_number=" + gap(2));
console.log("gap_number_wrapper=" + gap(new Number(2)));
console.log("gap_string_wrapper=" + gap(new String("--")));
console.log("gap_proxy_of_number_box=" + gap(new Proxy(new Number(2), {})));
console.log("gap_proxy_of_string_box=" + gap(new Proxy(new String("--"), {})));
console.log("gap_valueOf_object=" + gap({ valueOf() { return 2; } }));
console.log("gap_toString_object=" + gap({ toString() { return "--"; } }));
console.log("gap_is_compact_for_objects=" + (String((JSON.stringify as any)({ a: 1 }, null, {})) === compact));
console.log("gap_bigint=" + gap(2n));
console.log("gap_symbol=" + gap(Symbol("s")));

// --- the SLOT decides whether to unwrap, but the unwrapping itself is
//     ToNumber, so a patched valueOf on the box is honoured ---
const patched: any = new Number(2);
patched.valueOf = function () { return 8; };
console.log("gap_patched_valueOf=" + gap(patched));

// --- both slots are read once, before the walk starts ---
let listReads = 0;
const countingList: any = new Proxy(["a"], {
  get(t: any, k: any, r: any) { if (k === "0") listReads++; return Reflect.get(t, k, r); },
});
console.log("counted_list=" + s(countingList) + ":reads=" + listReads);

// --- stringify used as a mapper receives the index as replacer and the
//     array as gap, and ignores both ---
console.log("map_stringify=" + [1, "two", { a: 3 }].map(JSON.stringify as any).join("|"));
console.log("map_parse=" + JSON.stringify(['{"a":1}', "[2]"].map(JSON.parse as any)));
console.log("map_parse_reviver_index_ignored=" + JSON.stringify(["[1]"].map(JSON.parse as any)));
