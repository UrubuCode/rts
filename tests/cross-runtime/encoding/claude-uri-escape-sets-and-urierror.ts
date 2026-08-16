// Cross-runtime: the four URI functions differ only in which characters they
// leave alone. This pins the exact unreserved/reserved split of each one, the
// UTF-8 they emit, and the URIError a lone surrogate or a bad escape raises.

const ascii = function (): string {
  let s = "";
  for (let i = 0x21; i < 0x7f; i++) s += String.fromCharCode(i);
  return s;
};
const untouched = function (fn: (s: string) => string): string {
  let out = "";
  const src = ascii();
  for (let i = 0; i < src.length; i++) {
    if (fn(src[i]) === src[i]) out += src[i];
  }
  return out;
};

console.log("euc_keeps=" + untouched(encodeURIComponent));
console.log("eu_keeps=" + untouched(encodeURI));
console.log("esc_keeps=" + untouched(escape));
console.log("euc_minus_eu=" + (function (): string {
  let out = "";
  const src = ascii();
  for (let i = 0; i < src.length; i++) {
    if (encodeURI(src[i]) === src[i] && encodeURIComponent(src[i]) !== src[i]) out += src[i];
  }
  return out;
})());

// Space and the ASCII controls.
console.log("space=" + encodeURIComponent(" ") + "," + encodeURI(" ") + "," + escape(" "));
console.log("newline=" + encodeURIComponent("\n") + "," + escape("\n"));
console.log("plus=" + encodeURIComponent("+") + " (never a space here)");

// Multi-byte characters become percent-encoded UTF-8 by both encoders.
console.log("latin=" + encodeURIComponent("é") + "," + encodeURI("é"));
console.log("euro=" + encodeURIComponent("€"));
console.log("astral=" + encodeURIComponent("\u{1F600}"));
console.log("mixed=" + encodeURI("https://é.example/p a?q=é#f"));

// escape/unescape are the OLD pair: UTF-16 units, %uXXXX, not UTF-8.
console.log("escape_latin=" + escape("é"));
console.log("escape_euro=" + escape("€"));
console.log("escape_astral=" + escape("\u{1F600}"));
console.log("unescape_u=" + unescape("%u20AC") + " " + (unescape("%u20AC") === "€"));
console.log("unescape_hex=" + unescape("%E9") + " " + (unescape("%E9") === "é"));
console.log("unescape_passthrough=" + unescape("%zz") + "|" + unescape("%") + "|" + unescape("%u12"));
console.log("escape_roundtrip=" + (unescape(escape("aé€\u{1F600}")) === "aé€\u{1F600}"));

// decodeURIComponent undoes everything; decodeURI keeps the reserved set
// encoded, so it is NOT the inverse of encodeURIComponent.
const reserved = "%23%24%26%2B%2C%2F%3A%3B%3D%3F%40";
console.log("duc_reserved=" + decodeURIComponent(reserved));
console.log("du_reserved=" + decodeURI(reserved));
console.log("du_unreserved=" + decodeURI("%41%20%C3%A9"));
console.log("duc_utf8=" + decodeURIComponent("%E2%82%AC%F0%9F%98%80"));
console.log("not_inverse=" + (decodeURI(encodeURIComponent("a/b")) === "a/b"));
console.log("is_inverse=" + (decodeURIComponent(encodeURIComponent("a/b?c#d")) === "a/b?c#d"));

// Malformed sequences raise a URIError.
const malformed: string[] = ["%", "%z", "%zz", "%4", "%C3", "%C3%28", "%C0%80", "%E0%A4", "%F5%80%80%80", "%ED%A0%80", "%FF"];
for (const s of malformed) {
  let a = "no-throw";
  let b = "no-throw";
  try {
    decodeURIComponent(s);
  } catch (e: any) {
    a = e.constructor.name;
  }
  try {
    decodeURI(s);
  } catch (e: any) {
    b = e.constructor.name;
  }
  console.log("bad[" + s + "] duc=" + a + " du=" + b);
}

// A lone surrogate cannot be encoded as UTF-8.
const lone: string[] = ["\uD800", "\uDC00", "a\uD800b", "\uD800\uD800"];
for (const s of lone) {
  let a = "no-throw";
  try {
    encodeURIComponent(s);
  } catch (e: any) {
    a = e.constructor.name;
  }
  console.log("lone[" + (s.codePointAt(0) as number).toString(16) + ":" + s.length + "] euc=" + a + " escape=" + escape(s));
}
console.log("valid_pair=" + encodeURIComponent("😀"));

// Non-string arguments go through ToString first.
console.log("coerce=" + encodeURIComponent(1.5 as any) + "," + encodeURIComponent(null as any) + "," + encodeURIComponent(undefined as any));
console.log("coerce_object=" + encodeURIComponent({} as any));
console.log("coerce_array=" + encodeURIComponent(["a b", "c"] as any));

// Names and arities, since these are the oldest globals in the language.
console.log("arity=" + encodeURI.length + "," + encodeURIComponent.length + "," + decodeURI.length + "," + decodeURIComponent.length + "," + escape.length + "," + unescape.length);
console.log("names=" + [encodeURI.name, encodeURIComponent.name, decodeURI.name, decodeURIComponent.name, escape.name, unescape.name].join(","));
