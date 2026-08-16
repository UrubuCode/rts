// Cross-runtime: TextEncoder never splits a code point. encodeInto stops at the
// last WHOLE character that fits and reports {read, written} in UTF-16 units and
// bytes respectively; a lone surrogate becomes U+FFFD rather than an error.

const enc = new TextEncoder();
const hex = function (u: Uint8Array): string {
  const out: string[] = [];
  for (let i = 0; i < u.length; i++) out.push(u[i].toString(16).padStart(2, "0"));
  return out.join(" ");
};

console.log("encoding=" + enc.encoding);
console.log("tag=" + Object.prototype.toString.call(enc));

// "a" + U+1F600 (a surrogate pair, 4 bytes) + "b": the pair is all or nothing.
const text = "a\u{1F600}b";
console.log("source_units=" + text.length);
for (let size = 0; size <= 6; size++) {
  const dest = new Uint8Array(size);
  const r = enc.encodeInto(text, dest);
  console.log("into[" + size + "] read=" + r.read + " written=" + r.written + " bytes=" + hex(dest));
}

// A 3-byte character behaves the same way at every partial size.
const three = "€€";
for (let size = 1; size <= 4; size++) {
  const dest = new Uint8Array(size);
  const r = enc.encodeInto(three, dest);
  console.log("euro[" + size + "] read=" + r.read + " written=" + r.written + " bytes=" + hex(dest));
}

// encodeInto writes into the view's window and leaves the rest alone.
const backing = new Uint8Array(6).fill(0xaa);
const window = backing.subarray(2, 5);
const wr = enc.encodeInto("xy", window);
console.log("window read=" + wr.read + " written=" + wr.written + " backing=" + hex(backing));

// encode() always allocates exactly what is needed.
console.log("ascii=" + hex(enc.encode("AB")));
console.log("two_byte=" + hex(enc.encode("é")));
console.log("three_byte=" + hex(enc.encode("€")));
console.log("four_byte=" + hex(enc.encode("\u{1F600}")));
console.log("empty=" + enc.encode("").length + " kind=" + enc.encode("").constructor.name);
console.log("no_arg=" + enc.encode().length);
console.log("nonstring=" + hex(enc.encode(12 as any)) + "," + hex(enc.encode(null as any)));

// Lone surrogates: replaced with U+FFFD (ef bf bd), one per lone unit.
console.log("lone_high=" + hex(enc.encode("\uD83D")));
console.log("lone_low=" + hex(enc.encode("\uDE00")));
console.log("lone_in_middle=" + hex(enc.encode("a\uD800b")));
console.log("two_highs=" + hex(enc.encode("\uD800\uD800")));
console.log("reversed_pair=" + hex(enc.encode("\uDE00\uD83D")));
console.log("pair_then_lone=" + hex(enc.encode("\u{1F600}\uD800")));
console.log("valid_pair=" + hex(enc.encode("😀")));

// A pair split across two encode() calls cannot be rejoined — the encoder is
// stateless, unlike the decoder's streaming mode.
const firstHalf = enc.encode("\uD83D");
const secondHalf = enc.encode("\uDE00");
console.log("split_calls=" + hex(firstHalf) + " | " + hex(secondHalf));
console.log("split_total=" + (firstHalf.length + secondHalf.length) + " joined=" + enc.encode("😀").length);

// encodeInto with a lone surrogate still writes the replacement.
const dest3 = new Uint8Array(3);
const lr = enc.encodeInto("\uD800", dest3);
console.log("into_lone read=" + lr.read + " written=" + lr.written + " bytes=" + hex(dest3));
const dest2 = new Uint8Array(2);
const lr2 = enc.encodeInto("\uD800", dest2);
console.log("into_lone_short read=" + lr2.read + " written=" + lr2.written + " bytes=" + hex(dest2));

// The result is a plain object with exactly the two properties.
const res = enc.encodeInto("a", new Uint8Array(1));
console.log("result_keys=" + Object.keys(res).sort().join(","));
console.log("result_proto=" + (Object.getPrototypeOf(res) === Object.prototype));

// A round trip through the decoder is lossless for a valid pair and lossy for a
// lone surrogate.
const dec = new TextDecoder();
console.log("roundtrip_pair=" + (dec.decode(enc.encode("\u{1F600}")) === "\u{1F600}"));
console.log("roundtrip_lone=" + JSON.stringify(dec.decode(enc.encode("\uD800")).codePointAt(0) as any));
