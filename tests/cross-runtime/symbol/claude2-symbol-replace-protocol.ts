// Cross-runtime: Symbol.replace is the whole of String.prototype.replace and
// replaceAll. Any object carrying the hook takes the call over — the arguments
// it receives, the `this` it sees and the value it returns are passed straight
// through without a string in sight.

const seen: string[] = [];
const hook: any = {
  [Symbol.replace](str: any, repl: any) {
    seen.push("this_is_hook=" + (this === hook));
    seen.push("str=" + typeof str + ":" + String(str));
    const shape = typeof repl === "function" ? "fn:" + repl.name
      : (typeof repl === "object" && repl !== null) ? "obj" : String(repl);
    seen.push("repl=" + typeof repl + ":" + shape);
    return "REPLACED";
  },
};

// --- the hook wins over everything the string could have done ---
console.log("replace=" + "abcabc".replace(hook, "X"));
console.log("trace=" + seen.join("|"));

// --- the receiver reaches the hook UNCOERCED: ToString happens only on the
//     path the hook replaced ---
seen.length = 0;
console.log("number_receiver=" + String.prototype.replace.call(12345 as any, hook, "X"));
console.log("number_trace=" + seen.join("|"));

// --- the replacement value is NOT coerced ---
seen.length = 0;
const fn = function () { return "f"; };
"abc".replace(hook, fn as any);
console.log("fn_repl=" + seen[2]);
seen.length = 0;
"abc".replace(hook, { toString() { return "obj"; } } as any);
console.log("obj_repl=" + seen[2]);

// --- the return value is handed back verbatim, whatever it is ---
function returning(v: any): any {
  return "abc".replace({ [Symbol.replace]() { return v; } } as any, "X");
}
console.log("ret_number=" + typeof returning(42) + ":" + returning(42));
console.log("ret_null=" + String(returning(null)) + ":" + typeof returning(null));
console.log("ret_undefined=" + String(returning(undefined)));
console.log("ret_object=" + typeof returning({ a: 1 }));
console.log("ret_array=" + Array.isArray(returning([1, 2])));

// --- a hook that throws propagates ---
try {
  "abc".replace({ [Symbol.replace]() { throw new RangeError("hook"); } } as any, "X");
  console.log("throwing_hook=no_throw");
} catch (e: any) {
  console.log("throwing_hook=" + e.constructor.name);
}

// --- GetMethod: undefined and null mean "no hook", anything else must be callable ---
function withHook(v: any): string {
  const o: any = { toString() { return "b"; } };
  o[Symbol.replace] = v;
  try { return "abc".replace(o, "X"); }
  catch (e: any) { return e.constructor.name; }
}
console.log("hook_undefined=" + withHook(undefined));
console.log("hook_null=" + withHook(null));
console.log("hook_number=" + withHook(42));
console.log("hook_string=" + withHook("s"));
console.log("hook_object=" + withHook({}));

// --- the hook may be inherited ---
const proto: any = { [Symbol.replace]() { return "FROM_PROTO"; } };
console.log("inherited_hook=" + "abc".replace(Object.create(proto), "X"));

// --- a getter for the hook is read once per call ---
let reads = 0;
const lazy: any = {};
Object.defineProperty(lazy, Symbol.replace, {
  get() { reads++; return function () { return "LAZY" + reads; }; },
});
console.log("lazy1=" + "abc".replace(lazy, "X"));
console.log("lazy2=" + "abc".replace(lazy, "X"));
console.log("lazy_reads=" + reads);

// --- replaceAll routes through the same hook, but demands a global flag from
//     anything that LOOKS like a regular expression ---
const globalish: any = {
  [Symbol.match]: true,
  flags: "g",
  [Symbol.replace]() { return "ALL"; },
};
console.log("replaceAll_global=" + "abc".replaceAll(globalish, "X"));

const nonGlobalish: any = {
  [Symbol.match]: true,
  flags: "i",
  [Symbol.replace]() { return "NEVER"; },
};
try { "abc".replaceAll(nonGlobalish, "X"); console.log("replaceAll_nonglobal=no_throw"); }
catch (e: any) { console.log("replaceAll_nonglobal=" + e.constructor.name); }

// --- with no Symbol.match the flags are never consulted ---
const plainHook: any = { [Symbol.replace]() { return "PLAIN"; } };
console.log("replaceAll_no_match_brand=" + "abc".replaceAll(plainHook, "X"));

// --- a falsy Symbol.match also skips the check ---
const falsyMatch: any = { [Symbol.match]: 0, [Symbol.replace]() { return "FALSY"; } };
console.log("replaceAll_falsy_match=" + "abc".replaceAll(falsyMatch, "X"));

// --- but a PRESENT Symbol.match makes `flags` mandatory ---
const noFlags: any = { [Symbol.match]: true, [Symbol.replace]() { return "NOFLAGS"; } };
try { "abc".replaceAll(noFlags, "X"); console.log("replaceAll_missing_flags=no_throw"); }
catch (e: any) { console.log("replaceAll_missing_flags=" + e.constructor.name); }

// --- and the real RegExp hook still behaves as the built-in one ---
console.log("regexp_replace=" + "a-b-c".replace(/-/, "+"));
console.log("regexp_replace_global=" + "a-b-c".replace(/-/g, "+"));
console.log("regexp_replaceAll=" + "a-b-c".replaceAll(/-/g, "+"));
try { "a-b-c".replaceAll(/-/, "+"); console.log("regexp_replaceAll_nonglobal=no_throw"); }
catch (e: any) { console.log("regexp_replaceAll_nonglobal=" + e.constructor.name); }

// --- a RegExp whose own Symbol.replace is shadowed uses the shadow ---
const re: any = /-/g;
re[Symbol.replace] = function () { return "SHADOWED"; };
console.log("shadowed_regexp=" + "a-b".replace(re, "+"));
console.log("shadowed_regexp_all=" + "a-b".replaceAll(re, "+"));

// --- the descriptor of the built-in hook on RegExp.prototype ---
const d: any = Object.getOwnPropertyDescriptor(RegExp.prototype, Symbol.replace);
console.log("regexp_hook=" + typeof d.value + ":" + d.value.name + ":" + d.value.length);
console.log("regexp_hook_flags=" + d.writable + ":" + d.enumerable + ":" + d.configurable);
console.log("string_replace_arity=" + String.prototype.replace.length + ":" + String.prototype.replaceAll.length);

// --- with no hook anywhere the plain string search runs ---
console.log("plain_string=" + "a-b-c".replace("-", "+"));
console.log("plain_string_all=" + "a-b-c".replaceAll("-", "+"));
console.log("plain_coerced_search=" + "a1b".replace(1 as any, "+"));
