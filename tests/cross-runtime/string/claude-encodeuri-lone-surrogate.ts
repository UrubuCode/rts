// Cross-runtime: encodeURIComponent throws a URIError on a LONE surrogate — it
// is the one place where an ill-formed string stops being merely odd and becomes
// an error — and toWellFormed is the documented repair. Also pins the exact
// unreserved sets of encodeURI vs encodeURIComponent and the decode failures.
// 91/claude-towellformed cover isWellFormed but never reach the URI functions.

const LEAD = "\uD800";
const TRAIL = "\uDC00";
const PAIR = LEAD + TRAIL; // U+10000

function attempt(fn: () => string): string {
  try {
    return fn();
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

// --- well-formed input encodes as UTF-8 ---
console.log("ascii=" + encodeURIComponent("abc"));
console.log("latin1=" + encodeURIComponent("é"));
console.log("bmp=" + encodeURIComponent("中"));
console.log("pair=" + encodeURIComponent(PAIR));
console.log("emoji=" + encodeURIComponent("\u{1F600}"));

// --- a lone surrogate is a URIError, in both functions ---
console.log("euc-lead=" + attempt(() => encodeURIComponent("a" + LEAD + "b")));
console.log("euc-trail=" + attempt(() => encodeURIComponent("a" + TRAIL + "b")));
console.log("eu-lead=" + attempt(() => encodeURI("a" + LEAD + "b")));
console.log("euc-reversed=" + attempt(() => encodeURIComponent(TRAIL + LEAD)));
console.log("euc-lone-only=" + attempt(() => encodeURIComponent(LEAD)));

// --- toWellFormed repairs it into U+FFFD, which encodes fine ---
const broken = "a" + LEAD + "b";
console.log("isWellFormed=" + broken.isWellFormed());
console.log("repaired-len=" + broken.toWellFormed().length);
console.log("repaired-code=" + broken.toWellFormed().charCodeAt(1).toString(16));
console.log("euc-repaired=" + encodeURIComponent(broken.toWellFormed()));
console.log("pair-wellformed=" + PAIR.isWellFormed());
console.log("pair-unchanged=" + (PAIR.toWellFormed() === PAIR));

// --- the repair is a round trip through decodeURIComponent ---
const encoded = encodeURIComponent(broken.toWellFormed());
console.log("roundtrip=" + (decodeURIComponent(encoded) === broken.toWellFormed()));
console.log("roundtrip-len=" + decodeURIComponent(encoded).length);

// --- unreserved sets: encodeURI keeps the URI syntax characters ---
console.log("eu-reserved=" + encodeURI(";/?:@&=+$,#"));
console.log("euc-reserved=" + encodeURIComponent(";/?:@&=+$,#"));
console.log("eu-marks=" + encodeURI("-_.!~*'()"));
console.log("euc-marks=" + encodeURIComponent("-_.!~*'()"));
console.log("eu-space=" + encodeURI(" "));
console.log("euc-space=" + encodeURIComponent(" "));
console.log("eu-brackets=" + encodeURI("[]"));
console.log("euc-brackets=" + encodeURIComponent("[]"));
console.log("eu-percent=" + encodeURI("%"));

// --- escape/unescape are not involved: decodeURI keeps reserved escapes ---
console.log("du-keeps=" + decodeURI("%2F%3F"));
console.log("duc-decodes=" + decodeURIComponent("%2F%3F"));
console.log("du-space=" + decodeURI("%20"));

// --- malformed percent sequences are URIErrors ---
console.log("dec-truncated=" + attempt(() => decodeURIComponent("%E0%A4%A")));
console.log("dec-bad-hex=" + attempt(() => decodeURIComponent("%ZZ")));
console.log("dec-lone-percent=" + attempt(() => decodeURIComponent("%")));
console.log("dec-invalid-utf8=" + attempt(() => decodeURIComponent("%FF")));
console.log("dec-orphan-cont=" + attempt(() => decodeURIComponent("%80")));
console.log("dec-overlong=" + attempt(() => decodeURIComponent("%C0%AF")));
console.log("dec-surrogate=" + attempt(() => decodeURIComponent("%ED%A0%80")));
console.log("dec-plain=" + decodeURIComponent("a%62c"));

// --- arguments are coerced to strings first ---
console.log("num=" + encodeURIComponent(42 as any));
console.log("undef=" + encodeURIComponent(undefined as any));
console.log("arr=" + encodeURIComponent(["a", "b"] as any));
console.log("empty=" + "[" + encodeURIComponent("") + "]");
