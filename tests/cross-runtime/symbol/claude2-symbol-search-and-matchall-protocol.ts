// Cross-runtime: Symbol.search and Symbol.matchAll are dispatch points, not
// helpers. String.prototype.search/matchAll look the hook up on the argument
// and hand back whatever it answers; with no hook they build a RegExp from the
// argument and use ITS hook instead.

const searchTrace: string[] = [];
const searcher: any = {
  [Symbol.search](str: any) {
    searchTrace.push("this=" + (this === searcher) + ",arg=" + typeof str + ":" + String(str) + ",count=" + arguments.length);
    return 7;
  },
};

// --- search delegates and returns the hook's value untouched ---
console.log("search=" + "abcdef".search(searcher));
console.log("search_trace=" + searchTrace.join("|"));
console.log("search_type=" + typeof "abc".search(searcher));
console.log("search_string_ret=" + "abc".search({ [Symbol.search]() { return "not a number"; } } as any));
console.log("search_object_ret=" + typeof "abc".search({ [Symbol.search]() { return { a: 1 }; } } as any));
console.log("search_undefined_ret=" + String("abc".search({ [Symbol.search]() { return undefined; } } as any)));

// --- the receiver reaches the hook uncoerced ---
searchTrace.length = 0;
String.prototype.search.call(9876 as any, searcher);
console.log("search_receiver=" + searchTrace.join("|"));

// --- with no hook the argument becomes a RegExp ---
console.log("search_string_arg=" + "abcdef".search("cd"));
console.log("search_regexp_arg=" + "abcdef".search(/e/));
console.log("search_missing=" + "abcdef".search(/zz/));
console.log("search_no_arg=" + "abcdef".search());
console.log("search_undefined_arg=" + "abcdef".search(undefined as any));
console.log("search_null_arg=" + "aXnullY".search(null as any));
console.log("search_number_arg=" + "ab3cd".search(3 as any));

// --- search never advances lastIndex, even on a global regexp ---
const gre = /b/g;
gre.lastIndex = 4;
console.log("search_ignores_lastIndex=" + "abab".search(gre) + ":" + gre.lastIndex);

// --- a non-callable hook is a TypeError; null/undefined mean "no hook" ---
function withSearch(v: any): string {
  const o: any = { toString() { return "c"; } };
  o[Symbol.search] = v;
  try { return String("abc".search(o)); }
  catch (e: any) { return e.constructor.name; }
}
console.log("search_hook_undefined=" + withSearch(undefined));
console.log("search_hook_null=" + withSearch(null));
console.log("search_hook_number=" + withSearch(1));
console.log("search_hook_object=" + withSearch({}));

// --- matchAll delegates the same way ---
const allTrace: string[] = [];
const allHook: any = {
  [Symbol.matchAll](str: any) {
    allTrace.push("this=" + (this === allHook) + ",arg=" + String(str));
    return ["one", "two"];
  },
};
const allResult: any = "abc".matchAll(allHook);
console.log("matchAll_trace=" + allTrace.join("|"));
console.log("matchAll_returns_verbatim=" + Array.isArray(allResult) + ":" + allResult.join(","));
console.log("matchAll_non_iterable=" + String("abc".matchAll({ [Symbol.matchAll]() { return 5; } } as any)));

// --- but a regexp-LIKE argument must declare the global flag first ---
const nonGlobalish: any = { [Symbol.match]: true, flags: "i", [Symbol.matchAll]() { return "NEVER"; } };
try { "abc".matchAll(nonGlobalish); console.log("matchAll_nonglobal=no_throw"); }
catch (e: any) { console.log("matchAll_nonglobal=" + e.constructor.name); }

const globalish: any = { [Symbol.match]: true, flags: "gi", [Symbol.matchAll]() { return "OK"; } };
console.log("matchAll_globalish=" + "abc".matchAll(globalish));

const noMatchBrand: any = { [Symbol.matchAll]() { return "NOBRAND"; } };
console.log("matchAll_no_brand=" + "abc".matchAll(noMatchBrand));

const missingFlags: any = { [Symbol.match]: true, [Symbol.matchAll]() { return "NOFLAGS"; } };
try { "abc".matchAll(missingFlags); console.log("matchAll_missing_flags=no_throw"); }
catch (e: any) { console.log("matchAll_missing_flags=" + e.constructor.name); }

// --- a real non-global RegExp is refused for the same reason ---
try { "aXbX".matchAll(/X/); console.log("matchAll_regexp_nonglobal=no_throw"); }
catch (e: any) { console.log("matchAll_regexp_nonglobal=" + e.constructor.name); }

// --- with no hook, a plain string is turned into a GLOBAL regexp ---
const fromString = [..."aXbXc".matchAll("X" as any)];
console.log("matchAll_string_count=" + fromString.length);
console.log("matchAll_string_indices=" + fromString.map((m: any) => m.index).join(","));
console.log("matchAll_string_zero=" + fromString.map((m: any) => m[0]).join(","));
console.log("matchAll_undefined_arg_count=" + [..."ab".matchAll(undefined as any)].length);

// --- the iterator matchAll hands back is lazy and one-shot ---
const it: any = "aXbX".matchAll(/X/g);
console.log("matchAll_tag=" + Object.prototype.toString.call(it));
console.log("matchAll_self_iterable=" + (it[Symbol.iterator]() === it));
console.log("matchAll_first=" + it.next().value[0]);
console.log("matchAll_rest=" + [...it].length);
console.log("matchAll_exhausted=" + JSON.stringify(it.next()));

// --- matchAll does not disturb the source regexp's lastIndex ---
const src = /X/g;
src.lastIndex = 0;
const consumed = [..."aXbX".matchAll(src)];
console.log("matchAll_source_lastIndex=" + src.lastIndex + ":" + consumed.length);

// --- the hooks that RegExp.prototype actually carries ---
function hookShape(key: any, label: string): void {
  const d: any = Object.getOwnPropertyDescriptor(RegExp.prototype, key);
  console.log(label + "=" + typeof d.value + ":" + d.value.name + ":" + d.value.length + ":" + d.writable + d.enumerable + d.configurable);
}
hookShape(Symbol.search, "proto_search");
hookShape(Symbol.matchAll, "proto_matchAll");
hookShape(Symbol.match, "proto_match");
hookShape(Symbol.split, "proto_split");
console.log("string_arities=" + String.prototype.search.length + ":" + String.prototype.matchAll.length + ":" + String.prototype.match.length);

// --- a shadowed hook on the instance wins over the prototype's ---
const shadow: any = /X/g;
shadow[Symbol.search] = function () { return -99; };
shadow[Symbol.matchAll] = function () { return "SHADOW"; };
console.log("shadow_search=" + "aXb".search(shadow));
console.log("shadow_matchAll=" + "aXb".matchAll(shadow));
