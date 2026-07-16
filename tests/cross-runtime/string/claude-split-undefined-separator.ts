// Cross-runtime: split() with NO argument vs split(undefined) vs split("undefined").
// No-arg and undefined both yield the whole string as a single element (the
// separator is only "not present" when it is undefined). The literal STRING
// "undefined" is a real separator and does split.

const s = "a,b,c";

// --- no argument at all ---
const noArg = s.split();
console.log("noarg-len=" + noArg.length);
console.log("noarg-0=" + noArg[0]);
console.log("noarg-eq-src=" + (noArg[0] === s));

// --- explicit undefined behaves the same as no argument ---
const undef = s.split(undefined);
console.log("undef-len=" + undef.length);
console.log("undef-0=" + undef[0]);

// --- the literal string "undefined" IS a separator ---
const lit = "aundefinedb".split("undefined");
console.log("literal-len=" + lit.length);
console.log("literal=" + lit.join("|"));

// --- a variable holding undefined, not the literal ---
const sep: string | undefined = undefined;
console.log("var-undef-len=" + s.split(sep as any).length);

// --- null is NOT undefined: it coerces to the string "null" ---
const nul = "anullb".split(null as any);
console.log("null-len=" + nul.length);
console.log("null=" + nul.join("|"));
// and a string without "null" in it stays whole
console.log("null-nomatch=" + "abc".split(null as any).join("|"));

// --- undefined separator with a limit: limit still applies ---
console.log("undef-limit0-len=" + s.split(undefined, 0).length);
console.log("undef-limit1-len=" + s.split(undefined, 1).length);
console.log("undef-limit1-0=" + s.split(undefined, 1)[0]);
console.log("noarg-limit0-len=" + s.split(undefined as any, 0).length);

// --- empty source string, undefined separator: one empty element ---
const emptySrc = "".split(undefined);
console.log("empty-undef-len=" + emptySrc.length);
console.log("empty-undef-0=[" + emptySrc[0] + "]");

// --- contrast with the empty-string separator on the same source ---
console.log("empty-sep-len=" + "".split("").length);

// --- undefined vs a separator that is absent from the string ---
console.log("absent-len=" + s.split(";").length);
console.log("absent-0=" + s.split(";")[0]);

// --- the result is a fresh array, not the source ---
console.log("is-array=" + Array.isArray(s.split(undefined)));
console.log("elem-type=" + typeof s.split(undefined)[0]);
