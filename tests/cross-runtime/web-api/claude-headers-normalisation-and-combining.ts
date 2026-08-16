// Cross-runtime: a Headers list stores names case-insensitively, joins repeated
// values with ", ", trims surrounding whitespace from a value, iterates ordinary
// names in sorted order, and refuses a name or value outside the HTTP grammar
// with a TypeError.

const show = function (label: string, h: Headers): void {
  const parts: string[] = [];
  h.forEach(function (value, key) {
    parts.push(key + ": " + value);
  });
  console.log(label + " [" + parts.join(" | ") + "]");
};

// Names fold to lower case whatever the input spelling.
const h = new Headers();
h.append("Content-Type", "text/plain");
h.append("X-CUSTOM", "one");
h.append("x-custom", "two");
h.append("Accept", "a/b");
console.log("get_by_any_case=" + h.get("CONTENT-TYPE") + " | " + h.get("content-type") + " | " + h.get("Content-Type"));
console.log("combined=" + h.get("x-custom"));
console.log("has_any_case=" + h.has("X-Custom") + "," + h.has("x-custom") + "," + h.has("missing"));
show("sorted", h);
console.log("keys=" + [...h.keys()].join(","));
console.log("values=" + [...h.values()].join(" / "));
console.log("entries=" + [...h.entries()].map(function (p) { return p[0] + "=" + p[1]; }).join(" / "));
console.log("spread_matches_entries=" + (JSON.stringify([...h]) === JSON.stringify([...h.entries()])));

// A repeated append keeps adding to the same slot.
h.append("x-custom", "three");
console.log("three_values=" + h.get("x-custom"));
console.log("count_still_one_key=" + [...h.keys()].filter(function (k) { return k === "x-custom"; }).length);

// set() replaces every value at once and keeps the sorted position.
h.set("X-Custom", "only");
console.log("after_set=" + h.get("x-custom"));
show("after_set_order", h);
h.set("A-First", "1");
show("after_new_set", h);

// delete() is case-insensitive and silent when the name is absent.
h.delete("A-FIRST");
h.delete("not-there");
console.log("after_delete=" + [...h.keys()].join(","));

// Values are trimmed of leading and trailing HTTP whitespace, never inside.
const t = new Headers();
t.append("a", "  spaced  ");
t.append("b", "\ttabbed\t");
t.append("c", "in  ner");
console.log("trimmed=" + JSON.stringify(t.get("a")) + "," + JSON.stringify(t.get("b")) + "," + JSON.stringify(t.get("c")));
t.append("a", "  more  ");
console.log("trimmed_join=" + JSON.stringify(t.get("a")));
console.log("empty_value=" + JSON.stringify(new Headers({ e: "" }).get("e")));

// Non-string arguments are coerced with ToString.
const c = new Headers();
c.append("x-num", 42 as any);
c.append("x-bool", true as any);
console.log("coerced=" + c.get("x-num") + "," + c.get("x-bool"));

// Illegal names throw; the list is unchanged.
const badNames: string[] = ["a b", "a:b", "", "a\nb", "a\tb", "a(b", "a,b", "a@b", "a{b", "a\"b", "a/b", "a[b"];
for (const name of badNames) {
  try {
    new Headers().set(name, "1");
    console.log("name[" + JSON.stringify(name) + "]=accepted");
  } catch (e: any) {
    console.log("name[" + JSON.stringify(name) + "]=" + e.constructor.name);
  }
}
const goodNames: string[] = ["a-b", "a_b", "a.b", "a~b", "a!b", "a#b", "a$b", "a%b", "a&b", "a'b", "a*b", "a+b", "a^b", "a`b", "a|b", "0"];
console.log("accepted_names=" + goodNames.filter(function (n) {
  try {
    new Headers().set(n, "1");
    return true;
  } catch (e: any) {
    return false;
  }
}).join(","));

// Illegal values throw too.
const badValues: string[] = ["v\n1", "v\r1", "v\u00001", "\nv", "v\u0000", "\u0000v"];
for (const value of badValues) {
  try {
    new Headers().set("x", value);
    console.log("value[" + JSON.stringify(value) + "]=accepted");
  } catch (e: any) {
    console.log("value[" + JSON.stringify(value) + "]=" + e.constructor.name);
  }
}
console.log("value_with_inner_tab=" + JSON.stringify(new Headers({ x: "a\tb" }).get("x")));

// get() on a missing name answers null, not undefined.
const empty = new Headers();
console.log("missing=" + String(empty.get("nope")) + " typeof=" + typeof empty.get("nope"));
console.log("empty_iteration=" + JSON.stringify([...empty]) + " size_via_keys=" + [...empty.keys()].length);

// Construction from a record, an array of pairs, and another Headers.
show("from_record", new Headers({ "Z-Last": "1", "A-First": "2" }));
show("from_pairs", new Headers([["z-last", "1"], ["a-first", "2"], ["z-last", "3"]]));
show("from_headers", new Headers(new Headers({ b: "2", a: "1" })));
try {
  new Headers([["only-one"] as any]);
  console.log("bad_pair=accepted");
} catch (e: any) {
  console.log("bad_pair=" + e.constructor.name);
}
console.log("tag=" + Object.prototype.toString.call(h));
console.log("iterator_is_entries=" + ((Headers.prototype as any)[Symbol.iterator] === Headers.prototype.entries));
