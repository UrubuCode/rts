// Cross-runtime: `source` is NOT the pattern you passed — EscapeRegExpPattern
// rewrites it so that /<source>/ is always a re-parseable literal: a bare `/`
// becomes `\/`, and every line terminator becomes an escape sequence. 250 checks
// only patterns that survive unchanged. This pins the rewrite and the round trip
// back through the constructor.

function codes(s: string): string {
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join(",");
}

function show(label: string, pattern: string, flags: string): void {
  const re = new RegExp(pattern, flags);
  const src = re.source;
  const back = new RegExp(src, flags);
  console.log(
    label +
      " src=" + src +
      " len=" + src.length +
      " str=" + String(re) +
      " roundtrip=" + (back.source === src),
  );
}

// --- the forward slash: escaped even though the constructor got it bare ---
show("slash", "/", "");
show("slash-mid", "a/b", "");
show("slash-escaped", "\\/", "");
show("slash-in-class", "[/]", "");
show("many-slashes", "///", "");

// --- line terminators become escapes, so source is always one line ---
const LF = String.fromCharCode(0x0a);
const CR = String.fromCharCode(0x0d);
const LS = String.fromCharCode(0x2028);
const PS = String.fromCharCode(0x2029);
console.log("lf-src=" + JSON.stringify(new RegExp("a" + LF + "b").source));
console.log("lf-codes=" + codes(new RegExp(LF).source));
console.log("lf-len=" + new RegExp(LF).source.length);
console.log("cr-codes=" + codes(new RegExp(CR).source));
console.log("ls-codes=" + codes(new RegExp(LS).source));
console.log("ps-codes=" + codes(new RegExp(PS).source));
console.log("lf-matches=" + new RegExp(new RegExp(LF).source).test(LF));
console.log("ls-matches=" + new RegExp(new RegExp(LS).source).test(LS));

// --- other control characters are NOT rewritten: only the four terminators are ---
console.log("tab-codes=" + codes(new RegExp("\t").source));
console.log("nul-codes=" + codes(new RegExp(String.fromCharCode(0)).source));
console.log("ff-codes=" + codes(new RegExp("\f").source));

// --- the empty pattern is the one that has to invent a body ---
show("empty-ctor", "", "");
console.log("empty-literal-src=" + new RegExp("").source);
console.log("empty-literal-tostring=" + String(new RegExp("")));
console.log("empty-group-src=" + /(?:)/.source);
console.log("empty-equal=" + (new RegExp("").source === /(?:)/.source));
console.log("empty-matches=" + new RegExp("").test("anything"));
console.log("empty-roundtrip=" + new RegExp(new RegExp("").source).source);
console.log("proto-src=" + RegExp.prototype.source);

// --- everything else passes through verbatim, backslashes included ---
show("digit", "\\d+", "g");
show("backslash", "\\\\", "");
show("class", "[a-z/]", "i");
show("named", "(?<x>a)", "");
show("dollar", "$^", "");
show("unicode-escape", "\\u0041", "u");
show("brace", "a{2,3}", "");

// --- toString is /source/flags, and it round-trips through the constructor ---
const cases: RegExp[] = [/a/gi, /a\/b/m, new RegExp("x", "suy"), /(?:)/g, /[\]]/];
for (let i = 0; i < cases.length; i++) {
  const re = cases[i];
  const rebuilt = new RegExp(re.source, re.flags);
  console.log(
    "case" + i +
      " str=" + String(re) +
      " same=" + (String(rebuilt) === String(re)) +
      " flags=" + re.flags,
  );
}

// --- source is a PROTOTYPE ACCESSOR, not an own property ---
console.log("own-source=" + Object.prototype.hasOwnProperty.call(/a/, "source"));
const d: any = Object.getOwnPropertyDescriptor(RegExp.prototype, "source");
console.log("desc=" + (typeof d.get) + "/" + (d.set === undefined) + "/" + d.enumerable + "/" + d.configurable);
console.log("getter-name=" + d.get.name);
console.log("getter-length=" + d.get.length);

// --- copying source onto a plain object does not make it a regex ---
const fake: any = { source: "a", flags: "g" };
console.log("fake-tostring=" + RegExp.prototype.toString.call(fake));
console.log("fake-source-getter=" + (function () {
  try {
    return String(d.get.call(fake));
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
})());
