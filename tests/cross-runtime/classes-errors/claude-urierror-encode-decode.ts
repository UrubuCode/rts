// Cross-runtime: URIError is raised by the four URI functions for a malformed
// escape sequence or a lone surrogate, and by nothing else — the successful
// cases pin the reserved-character split between the encode/decode pairs.
function probe(fn: () => any): string {
  try {
    return "ok:" + String(fn());
  } catch (e: any) {
    return e.constructor.name + ":" + String(e instanceof URIError) + ":" + String(e instanceof Error);
  }
}

console.log("decodeuric-percent=" + probe(() => decodeURIComponent("%")));
console.log("decodeuric-short=" + probe(() => decodeURIComponent("%2")));
console.log("decodeuric-nonhex=" + probe(() => decodeURIComponent("%zz")));
console.log("decodeuric-trailing=" + probe(() => decodeURIComponent("a%")));
console.log("decodeuric-bad-utf8=" + probe(() => decodeURIComponent("%C0%80")));
console.log("decodeuric-lone-cont=" + probe(() => decodeURIComponent("%80")));
console.log("decodeuric-truncated=" + probe(() => decodeURIComponent("%E2%82")));

console.log("decodeuri-percent=" + probe(() => decodeURI("%")));
console.log("decodeuri-nonhex=" + probe(() => decodeURI("%GG")));

console.log("encodeuric-lone-high=" + probe(() => encodeURIComponent("\uD800")));
console.log("encodeuric-lone-low=" + probe(() => encodeURIComponent("\uDC00")));
console.log("encodeuric-reversed=" + probe(() => encodeURIComponent("\uDC00\uD800")));
console.log("encodeuri-lone-high=" + probe(() => encodeURI("\uD800")));

// A well-formed surrogate pair encodes fine.
console.log("encodeuric-pair=" + probe(() => encodeURIComponent("😀")));
console.log("decodeuric-pair=" + probe(() => decodeURIComponent("%F0%9F%98%80").length));
console.log("roundtrip=" + probe(() => decodeURIComponent(encodeURIComponent("😀")).length));

// The successful surface: which characters each function leaves alone.
console.log("euric-reserved=" + encodeURIComponent(";/?:@&=+$,#"));
console.log("euri-reserved=" + encodeURI(";/?:@&=+$,#"));
console.log("euric-unreserved=" + encodeURIComponent("-_.!~*'()"));
console.log("euri-unreserved=" + encodeURI("-_.!~*'()"));
console.log("euric-space=" + encodeURIComponent(" "));
console.log("euri-space=" + encodeURI(" "));
console.log("euric-alnum=" + encodeURIComponent("aZ09"));
console.log("euric-accent=" + encodeURIComponent("é"));
console.log("euric-cjk=" + encodeURIComponent("中"));

console.log("duric-reserved=" + decodeURIComponent("%3B%2F%3F%3A%40%26%3D%2B%24%2C%23"));
console.log("duri-reserved=" + decodeURI("%3B%2F%3F%3A%40%26%3D%2B%24%2C%23"));
console.log("duri-space=" + decodeURI("%20"));
console.log("duri-percent-escape=" + decodeURI("%25"));

// escape/unescape (Annex B) never raise URIError.
console.log("unescape-percent=" + probe(() => JSON.stringify(unescape("%"))));
console.log("unescape-short=" + probe(() => JSON.stringify(unescape("%2"))));
console.log("escape-lone-surrogate=" + probe(() => escape("\uD800")));
console.log("escape-space=" + escape(" "));
console.log("unescape-u=" + unescape("%u0041"));

// Arguments are coerced with ToString first.
console.log("coerce-number=" + probe(() => encodeURIComponent(1 as any)));
console.log("coerce-null=" + probe(() => encodeURIComponent(null as any)));
console.log("coerce-undefined=" + probe(() => encodeURIComponent(undefined as any)));
console.log("coerce-array=" + probe(() => encodeURIComponent([1, 2] as any)));
console.log("coerce-symbol=" + probe(() => encodeURIComponent(Symbol("s") as any)));

// The functions are ordinary globals with fixed shapes.
console.log("euric-length=" + encodeURIComponent.length);
console.log("euri-length=" + encodeURI.length);
console.log("duric-length=" + decodeURIComponent.length);
console.log("duri-length=" + decodeURI.length);
console.log("euric-name=" + encodeURIComponent.name);
console.log("duri-name=" + decodeURI.name);
console.log("urierror-proto=" + (Object.getPrototypeOf(URIError.prototype) === Error.prototype));
console.log("urierror-name=" + URIError.prototype.name);
console.log("urierror-own-name=" + Object.prototype.hasOwnProperty.call(new URIError("x"), "name"));
console.log("urierror-tostring=" + new URIError("mine").toString());
