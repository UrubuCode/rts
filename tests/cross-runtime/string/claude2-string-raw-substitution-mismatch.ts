// Cross-runtime: String.raw reads `raw.length` and interleaves substitutions
// only BETWEEN raw segments — so a `raw` shorter than the substitution list
// silently drops the extras, a `raw` LONGER produces "undefined" from a missing
// substitution, and `raw` needs only to be array-LIKE, which a plain string
// satisfies. 197/codex2_033/codex_string_raw_arraylike use it as a tag; this
// pins the length arithmetic and the failure modes.

function attempt(f: () => any): string {
  try {
    return JSON.stringify(f());
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

const raw: any = String.raw;

// --- the normal tag use, for reference ---
console.log("tag-basic=" + JSON.stringify(String.raw`a\nb`));
console.log("tag-sub=" + JSON.stringify(String.raw`a${1}b${2}c`));
console.log("tag-empty=" + JSON.stringify(String.raw``));
console.log("tag-only-sub=" + JSON.stringify(String.raw`${1}`));

// --- called directly: raw.length decides how many substitutions are used ---
console.log("exact=" + attempt(() => raw({ raw: ["a", "b", "c"] }, 1, 2)));
console.log("extra-subs=" + attempt(() => raw({ raw: ["a", "b"] }, 1, 2, 3)));
console.log("one-segment=" + attempt(() => raw({ raw: ["a"] }, 1, 2)));
console.log("missing-subs=" + attempt(() => raw({ raw: ["a", "b", "c"] })));
console.log("one-sub-three-segs=" + attempt(() => raw({ raw: ["a", "b", "c"] }, "X")));
console.log("empty-raw=" + attempt(() => raw({ raw: [] }, 1, 2)));
console.log("empty-raw-nosubs=" + attempt(() => raw({ raw: [] })));

// --- the LAST segment never gets a substitution after it ---
console.log("trailing=" + attempt(() => raw({ raw: ["<", ">"] }, "MID")));
console.log("leading=" + attempt(() => raw({ raw: ["", "!"] }, "X")));
console.log("both-empty=" + attempt(() => raw({ raw: ["", ""] }, "X")));

// --- `raw` only has to be array-LIKE: a string works, and its units are segments ---
console.log("string-raw=" + attempt(() => raw({ raw: "xyz" }, 1, 2)));
console.log("string-raw-short=" + attempt(() => raw({ raw: "x" }, 1)));
console.log("string-raw-empty=" + attempt(() => raw({ raw: "" }, 1)));
console.log("arraylike=" + attempt(() => raw({ raw: { length: 2, 0: "a", 1: "b" } }, "-")));
console.log("holes=" + attempt(() => raw({ raw: { length: 3, 0: "a", 2: "c" } }, "-", "-")));
console.log("sparse-array=" + attempt(() => raw({ raw: [, "b"] as any }, "-")));

// --- length is ToLength'd, so a fractional or negative length is clamped ---
console.log("frac-length=" + attempt(() => raw({ raw: { length: 2.9, 0: "a", 1: "b", 2: "c" } }, "-", "-")));
console.log("neg-length=" + attempt(() => raw({ raw: { length: -1, 0: "a" } }, "-")));
console.log("str-length=" + attempt(() => raw({ raw: { length: "2", 0: "a", 1: "b" } }, "-")));
console.log("no-length=" + attempt(() => raw({ raw: { 0: "a", 1: "b" } }, "-")));

// --- every segment and every substitution is ToString'd ---
console.log("num-segments=" + attempt(() => raw({ raw: [1, 2] }, 9)));
console.log("obj-sub=" + attempt(() => raw({ raw: ["<", ">"] }, { toString: () => "T" })));
console.log("null-sub=" + attempt(() => raw({ raw: ["<", ">"] }, null)));
console.log("undefined-sub=" + attempt(() => raw({ raw: ["<", ">"] }, undefined)));
console.log("symbol-sub=" + attempt(() => raw({ raw: ["<", ">"] }, Symbol("s"))));
console.log("symbol-segment=" + attempt(() => raw({ raw: [Symbol("s")] })));
console.log("throwing-sub=" + attempt(() => raw({ raw: ["<", ">"] }, { toString: () => { throw new RangeError("x"); } })));

// --- a missing or non-object `raw` is where it finally throws ---
console.log("no-raw=" + attempt(() => raw({})));
console.log("raw-null=" + attempt(() => raw({ raw: null })));
console.log("raw-number=" + attempt(() => raw({ raw: 5 })));
console.log("no-arg=" + attempt(() => raw()));
console.log("null-arg=" + attempt(() => raw(null)));
console.log("raw-getter=" + attempt(() => raw({ get raw() { return ["g"]; } })));
console.log("raw-throwing-getter=" + attempt(() => raw({ get raw() { throw new RangeError("x"); } })));

// --- the function's own shape ---
console.log("name=" + String.raw.name);
console.log("length=" + String.raw.length);
console.log("is-own=" + Object.prototype.hasOwnProperty.call(String, "raw"));

// --- and the invariant that makes it useful: cooked escapes vs raw ones ---
console.log("cooked-vs-raw=" + JSON.stringify(`a\tb`) + " / " + JSON.stringify(String.raw`a\tb`));
console.log("raw-backslash-len=" + String.raw`\\`.length);
console.log("raw-unicode=" + JSON.stringify(String.raw`A`));
console.log("raw-crlf-len=" + String.raw`a
b`.length);
