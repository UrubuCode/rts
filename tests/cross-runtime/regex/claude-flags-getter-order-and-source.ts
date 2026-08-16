// Cross-runtime: `flags` is ASSEMBLED by a getter in a fixed order (dgimsuvy),
// never echoed back in the order written; and `source` is escaped so that
// `/${source}/` always re-parses — an empty pattern becomes "(?:)" and a literal
// newline becomes "\n". 170/250 read them but never in a scrambled order and
// never on the prototype itself.

// --- the order is canonical, not the source order ---
console.log("literal-scrambled=" + /a/yusimgd.flags);
console.log("literal-plain=" + /a/.flags);
console.log("ctor-scrambled=" + new RegExp("a", "yusimgd").flags);
console.log("ctor-reverse=" + new RegExp("a", "dgimsuy").flags);
console.log("v-instead-of-u=" + new RegExp("a", "yvsimgd").flags);
console.log("subset=" + /a/mi.flags);
console.log("subset2=" + /a/gi.flags);

// --- every individual getter ---
const all = /a/dgimsy;
console.log("bools=" + [all.hasIndices, all.global, all.ignoreCase, all.multiline,
  all.dotAll, all.sticky, all.unicode, all.unicodeSets].join(","));
const uni = /a/u;
console.log("u-bools=" + uni.unicode + "," + uni.unicodeSets);
const sets = /a/v;
console.log("v-bools=" + sets.unicode + "," + sets.unicodeSets);

// --- flags is an accessor on the prototype, not an own data property ---
console.log("own-flags=" + Object.prototype.hasOwnProperty.call(/a/g, "flags"));
console.log("own-source=" + Object.prototype.hasOwnProperty.call(/a/g, "source"));
const d: any = Object.getOwnPropertyDescriptor(RegExp.prototype, "flags");
console.log("desc-flags=" + (typeof d.get) + "/" + String(d.value) + "/" + d.enumerable + "/" + d.configurable);

// --- the prototype itself answers the sentinel values ---
console.log("proto-source=" + RegExp.prototype.source);
console.log("proto-flags=[" + RegExp.prototype.flags + "]");
console.log("proto-global=" + String(RegExp.prototype.global));
console.log("proto-tostring=" + RegExp.prototype.toString());

// --- flags can be read off a NON-regex through call, since it only reads props ---
const fake: any = { global: true, ignoreCase: false, multiline: true, dotAll: false,
  unicode: false, unicodeSets: false, sticky: true, hasIndices: false };
console.log("fake-flags=" + (d.get.call(fake) as string));

// --- source escapes whatever would break the /.../ round trip ---
console.log("empty-literal=" + new RegExp("").source);
console.log("empty-tostring=" + String(new RegExp("")));
console.log("empty-matches=" + new RegExp("").test("x"));
console.log("slash-in-class=" + /[/]/.source);
console.log("slash-in-class-str=" + String(/[/]/));
console.log("slash-escaped=" + /\//.source);
console.log("ctor-slash=" + new RegExp("/").source);
console.log("ctor-slash-str=" + String(new RegExp("/")));
console.log("newline=" + new RegExp("\n").source);
console.log("newline-str=" + String(new RegExp("\n")));
console.log("newline-len=" + new RegExp("\n").source.length);
console.log("newline-matches=" + new RegExp("\n").test("\n"));
console.log("cr=" + new RegExp("\r").source);
console.log("ls=" + new RegExp("\u2028").source);
console.log("ps=" + new RegExp("\u2029").source);
console.log("escaped-slash-kept=" + new RegExp("a\\/b").source);

// --- source is NOT re-escaped for characters that need no escape ---
console.log("dot=" + /a.b/.source);
console.log("backslash=" + /\\/.source.length);
console.log("class-caret=" + /[^a]/.source);
console.log("nul=" + new RegExp("\u0000").source.length);

// --- toString is exactly "/" + source + "/" + flags ---
const r = /a\/b/gi;
console.log("tostring-parts=" + (String(r) === "/" + r.source + "/" + r.flags));
console.log("tostring=" + String(r));
console.log("roundtrip=" + new RegExp(/[/]/.source).test("/"));
console.log("roundtrip-empty=" + new RegExp(new RegExp("").source).source);

// --- duplicated, unknown, and mutually exclusive flags are SyntaxErrors ---
function bad(src: string, f: string): string {
  try {
    return "ok:" + new RegExp(src, f).flags;
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}
console.log("dup=" + bad("a", "gg"));
console.log("unknown=" + bad("a", "q"));
console.log("uv=" + bad("a", "uv"));
console.log("uppercase=" + bad("a", "G"));
console.log("space=" + bad("a", " g"));
console.log("empty-flags=" + bad("a", ""));
console.log("undef-flags=" + "ok:[" + new RegExp("a", undefined).flags + "]");
