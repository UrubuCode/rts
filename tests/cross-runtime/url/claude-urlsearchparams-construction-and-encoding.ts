// Cross-runtime: every shape URLSearchParams accepts on the way in, and the
// application/x-www-form-urlencoded serialiser on the way out — where + means a
// space, which characters survive unescaped, and how empty names and values are
// represented.

const dump = function (label: string, p: URLSearchParams): void {
  const pairs: string[] = [];
  p.forEach(function (value, key) {
    pairs.push(JSON.stringify(key) + ":" + JSON.stringify(value));
  });
  console.log(label + " size=" + p.size + " str=" + JSON.stringify(p.toString()) + " pairs=[" + pairs.join(" ") + "]");
};

// From a string, with and without the leading question mark.
dump("plain", new URLSearchParams("a=1&b=2"));
dump("leading_mark", new URLSearchParams("?a=1&b=2"));
dump("two_marks", new URLSearchParams("??a=1"));
dump("empty_string", new URLSearchParams(""));
dump("only_mark", new URLSearchParams("?"));
dump("no_value", new URLSearchParams("a&b="));
dump("no_name", new URLSearchParams("=x&=y"));
dump("bare_equals", new URLSearchParams("="));
dump("empty_pairs", new URLSearchParams("a=1&&b=2&"));
dump("duplicate", new URLSearchParams("a=1&a=2&a=3"));
dump("extra_equals", new URLSearchParams("a=1=2"));

// From an array of pairs, an iterable of pairs, and a record.
dump("array", new URLSearchParams([["k", "v"], ["k", "w"], ["z", "1"]]));
dump("map", new URLSearchParams(new Map([["b", "2"], ["a", "1"]]) as any));
dump("record", new URLSearchParams({ b: "2", a: "1", "1": "first" }));
dump("record_dupes", new URLSearchParams({ a: "1", A: "2" }));
dump("from_params", new URLSearchParams(new URLSearchParams("a=1&a=2")));
dump("no_arg", new URLSearchParams());
dump("undefined_arg", new URLSearchParams(undefined));

// Values are coerced with ToString, including for a record.
dump("coerced_record", new URLSearchParams({ n: 1 as any, b: true as any, u: null as any }));
dump("coerced_array", new URLSearchParams([["n", 1 as any], ["u", undefined as any]]));

// A malformed input shape throws instead of being ignored.
const bad: Array<[string, () => URLSearchParams]> = [
  ["short_pair", function () { return new URLSearchParams([["a"] as any]); }],
  ["long_pair", function () { return new URLSearchParams([["a", "b", "c"] as any]); }],
  ["flat_array", function () { return new URLSearchParams(["a", "b"] as any); }],
  ["number", function () { return new URLSearchParams(5 as any); }],
];
for (const b of bad) {
  try {
    console.log("ctor_" + b[0] + "=" + JSON.stringify(b[1]().toString()));
  } catch (e: any) {
    console.log("ctor_" + b[0] + "=" + e.constructor.name);
  }
}

// Decoding: + is a space, %XX is a byte, and invalid escapes are left alone.
console.log("plus_decoded=" + JSON.stringify(new URLSearchParams("q=a+b").get("q")));
console.log("pct20_decoded=" + JSON.stringify(new URLSearchParams("q=a%20b").get("q")));
console.log("literal_plus=" + JSON.stringify(new URLSearchParams("q=a%2Bb").get("q")));
console.log("utf8_decoded=" + JSON.stringify(new URLSearchParams("q=%C3%A9%E2%82%AC").get("q")));
console.log("bad_escape=" + JSON.stringify(new URLSearchParams("q=%zz%").get("q")));
console.log("truncated_escape=" + JSON.stringify(new URLSearchParams("q=%C3").get("q")));
console.log("plus_in_name=" + JSON.stringify([...new URLSearchParams("a+b=1").keys()][0]));

// Serialising: + for space, %XX for everything outside the safe set.
const out = new URLSearchParams();
out.append("a b", "c d");
out.append("amp", "x&y=z");
out.append("plus", "1+1");
out.append("uni", "é€\u{1F600}");
out.append("safe", "*-._");
out.append("unsafe", "~!'()");
out.append("tilde", "~");
out.append("nl", "line1\nline2");
console.log("serialised=" + out.toString());
console.log("roundtrip=" + (function (): string {
  const back = new URLSearchParams(out.toString());
  const same: string[] = [];
  out.forEach(function (v, k) {
    same.push(String(back.getAll(k).indexOf(v) >= 0));
  });
  return same.join(",");
})());

// Empty name and empty value round-trip through the serialiser.
const edge = new URLSearchParams();
edge.append("", "v");
edge.append("k", "");
edge.append("", "");
console.log("edge=" + JSON.stringify(edge.toString()));
console.log("edge_back=" + JSON.stringify(new URLSearchParams(edge.toString()).toString()));

// The iterator protocol and the tag.
const iter = new URLSearchParams("a=1&b=2");
console.log("entries_is_iterator=" + (typeof (iter as any)[Symbol.iterator] === "function") + " same_as_entries=" + ((iter as any)[Symbol.iterator] === iter.entries));
console.log("keys=" + [...iter.keys()].join(",") + " values=" + [...iter.values()].join(","));
console.log("spread=" + JSON.stringify([...iter]));
console.log("tag=" + Object.prototype.toString.call(iter));
console.log("iterator_tag=" + Object.prototype.toString.call(iter.entries()));
console.log("string_coerce=" + String(iter));
