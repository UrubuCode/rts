// Cross-runtime: split's `limit` goes through ToUint32, not ToInteger — so -1
// means 4294967295 (no limit in practice) and 2^32 means 0 (empty array). A
// limit of 0 short-circuits BEFORE the separator is even consulted. Existing
// split fixtures only pass small non-negative integers.

function j(a: string[]): string {
  return "[" + a.join("|") + "]#" + a.length;
}

const csv = "a,b,c,d,e";

// --- ordinary limits ---
console.log("lim-und=" + j(csv.split(",", undefined)));
console.log("lim-noarg=" + j(csv.split(",")));
console.log("lim-1=" + j(csv.split(",", 1)));
console.log("lim-3=" + j(csv.split(",", 3)));
console.log("lim-99=" + j(csv.split(",", 99)));

// --- limit 0 gives the empty array for every separator ---
console.log("lim-0=" + j(csv.split(",", 0)));
console.log("lim-0-empty-sep=" + j("abc".split("", 0)));
console.log("lim-0-nomatch=" + j("abc".split("z", 0)));
console.log("lim-0-regex=" + j("abc".split(/b/, 0)));
console.log("lim-0-undef-sep=" + j("abc".split(undefined as any, 0)));

// --- ToUint32 wraps negatives around ---
console.log("lim-neg1=" + j(csv.split(",", -1)));
console.log("lim-neg5=" + j(csv.split(",", -5)));
console.log("lim-negzero=" + j(csv.split(",", -0)));

// --- ToUint32 of 2^32 and 2^32+3 ---
console.log("lim-2p32=" + j(csv.split(",", 4294967296)));
console.log("lim-2p32p3=" + j(csv.split(",", 4294967299)));
console.log("lim-2p32m1=" + j(csv.split(",", 4294967295)).length);

// --- non-numbers go through ToUint32 as well ---
console.log("lim-str2=" + j(csv.split(",", "2" as any)));
console.log("lim-str-bad=" + j(csv.split(",", "zz" as any)));
console.log("lim-nan=" + j(csv.split(",", NaN as any)));
console.log("lim-inf=" + j(csv.split(",", Infinity as any)));
console.log("lim-neginf=" + j(csv.split(",", -Infinity as any)));
console.log("lim-true=" + j(csv.split(",", true as any)));
console.log("lim-false=" + j(csv.split(",", false as any)));
console.log("lim-null=" + j(csv.split(",", null as any)));
console.log("lim-frac=" + j(csv.split(",", 2.9 as any)));
console.log("lim-negfrac=" + j(csv.split(",", -0.5 as any)));
const boxed: any = { valueOf() { return 2; } };
console.log("lim-obj=" + j(csv.split(",", boxed)));
console.log("lim-arr=" + j(csv.split(",", [3] as any)));

// --- the limit counts OUTPUT slots, and captures occupy slots too ---
console.log("cap-lim2=" + j("a1b2c".split(/(\d)/, 2)));
console.log("cap-lim3=" + j("a1b2c".split(/(\d)/, 3)));
console.log("cap-lim4=" + j("a1b2c".split(/(\d)/, 4)));

// --- an undefined separator ignores the limit's meaning but not limit 0 ---
console.log("undef-sep=" + j("abc".split(undefined as any)));
console.log("undef-sep-lim1=" + j("abc".split(undefined as any, 1)));

// --- limit on an empty-string separator over a surrogate pair ---
console.log("units-lim3=" + "a\u{1F600}b".split("", 3).length);
console.log("units-lim2=" + "a\u{1F600}b".split("", 2).join("").length);

// --- an empty subject with a limit ---
console.log("empty-lim0=" + j("".split(",", 0)));
console.log("empty-lim1=" + j("".split(",", 1)));
console.log("empty-empty-sep=" + j("".split("")));
