// Cross-runtime: Symbol.match is the language's "is this a regular expression?"
// question (IsRegExp). It is asked by startsWith/endsWith/includes, which
// REFUSE anything that answers yes, and by the RegExp constructor. A plain
// object can therefore be refused, and a real RegExp can be let through.

function probe(label: string, fn: () => any): void {
  try { console.log(label + "=ok:" + String(fn())); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}

// --- a real regular expression is refused by the three "is it in there" methods ---
probe("startsWith_regexp", () => "abc".startsWith(/a/ as any));
probe("endsWith_regexp", () => "abc".endsWith(/c/ as any));
probe("includes_regexp", () => "abc".includes(/b/ as any));

// --- so is a plain object that merely SAYS it is one ---
const pretender: any = { [Symbol.match]: true, toString() { return "b"; } };
probe("startsWith_pretender", () => "abc".startsWith(pretender));
probe("endsWith_pretender", () => "abc".endsWith(pretender));
probe("includes_pretender", () => "abc".includes(pretender));

// --- the answer is ToBoolean, so any truthy value refuses ---
function brandedWith(v: any): string {
  const o: any = { toString() { return "b"; } };
  o[Symbol.match] = v;
  try { return "abc".includes(o) ? "true" : "false"; }
  catch (e: any) { return e.constructor.name; }
}
console.log("brand_true=" + brandedWith(true));
console.log("brand_one=" + brandedWith(1));
console.log("brand_string=" + brandedWith("x"));
console.log("brand_object=" + brandedWith({}));
console.log("brand_empty_string=" + brandedWith(""));
console.log("brand_zero=" + brandedWith(0));
console.log("brand_false=" + brandedWith(false));
console.log("brand_null=" + brandedWith(null));
console.log("brand_nan=" + brandedWith(NaN));

// --- undefined means "not a regexp", so the object is coerced to a string ---
const plain: any = { toString() { return "b"; } };
console.log("no_brand_includes=" + "abc".includes(plain));
console.log("no_brand_startsWith=" + "bcd".startsWith(plain));

// --- and a real RegExp that DENIES the brand is let through and stringified ---
const denying: any = /b/;
denying[Symbol.match] = false;
console.log("denying_includes=" + "abc".includes(denying));
console.log("denying_source_text=" + "a/b/c".includes(denying));
console.log("denying_startsWith=" + "/b/x".startsWith(denying));
console.log("denying_still_regexp=" + (denying instanceof RegExp) + ":" + denying.test("abc"));

// --- the brand is read from the prototype chain too ---
const brandedProto: any = Object.create({ [Symbol.match]: true });
brandedProto.toString = function () { return "b"; };
probe("inherited_brand", () => "abc".includes(brandedProto));

// --- a getter is invoked, and its throw escapes ---
let reads = 0;
const counting: any = { toString() { return "b"; } };
Object.defineProperty(counting, Symbol.match, { get() { reads++; return false; } });
console.log("getter_includes=" + "abc".includes(counting) + ":reads=" + reads);
const throwing: any = {};
Object.defineProperty(throwing, Symbol.match, { get() { throw new EvalError("brand"); } });
probe("throwing_brand", () => "abc".includes(throwing));

// --- split does NOT ask the question: it dispatches on Symbol.split instead ---
console.log("split_regexp=" + "a1b2c".split(/\d/).join(","));
console.log("split_pretender=" + "abc".split(pretender).join("|"));
console.log("indexOf_pretender=" + "abc".indexOf(pretender));
console.log("lastIndexOf_pretender=" + "abcb".lastIndexOf(pretender));
console.log("concat_pretender=" + "a".concat(pretender));

// --- the RegExp constructor asks it as well: called as a FUNCTION with a
//     branded argument whose constructor is RegExp, it hands the argument back ---
const selfish: any = { [Symbol.match]: true, constructor: RegExp };
console.log("RegExp_returns_same=" + (RegExp(selfish) === selfish));
console.log("RegExp_new_is_fresh=" + (new RegExp(selfish) === selfish));
console.log("RegExp_new_typeof=" + (new RegExp(selfish) instanceof RegExp));

const otherCtor: any = { [Symbol.match]: true, constructor: function Other() { /* not RegExp */ }, source: "z", flags: "" };
console.log("RegExp_other_ctor_fresh=" + (RegExp(otherCtor) === otherCtor));
console.log("RegExp_other_ctor_source=" + RegExp(otherCtor).source);

const re = /ab/g;
console.log("RegExp_of_regexp_same=" + (RegExp(re) === re));
console.log("RegExp_of_regexp_with_flags=" + (RegExp(re, "i") === re) + ":" + RegExp(re, "i").flags);
console.log("RegExp_new_of_regexp=" + (new RegExp(re) === re) + ":" + new RegExp(re).flags);

// --- a branded object without source/flags reads them as undefined ---
const bare: any = { [Symbol.match]: true };
const built = new RegExp(bare);
console.log("branded_bare_source=" + built.source + ":flags=" + JSON.stringify(built.flags));

// --- the brand does not change what the object IS ---
console.log("pretender_typeof=" + typeof pretender + ":instanceof=" + (pretender instanceof RegExp));
console.log("pretender_tag=" + Object.prototype.toString.call(pretender));
console.log("regexp_own_match=" + Object.prototype.hasOwnProperty.call(RegExp.prototype, Symbol.match));
console.log("regexp_instance_own_match=" + Object.prototype.hasOwnProperty.call(/x/, Symbol.match));

// --- String.prototype.match uses the hook, and falls back to a fresh RegExp ---
console.log("match_hook=" + "abc".match({ [Symbol.match]() { return "HOOK"; } } as any));
console.log("match_string_fallback=" + JSON.stringify("abcb".match("b" as any)));
// String.prototype.match reads @@match as a METHOD, so the very value that
// makes IsRegExp answer yes — a bare `true` — is refused before any fallback:
// a brand good enough to be REJECTED by includes() cannot be used by match()
probe("match_brand_without_hook", () => JSON.stringify("abzc".match({ [Symbol.match]: true, toString() { return "z"; } } as any)));
probe("match_brand_with_flags", () => JSON.stringify("abzc".match({ [Symbol.match]: true, source: "z", flags: "" } as any)));
