// Cross-runtime: where each mutation lands in the ordered pair list — set()
// replaces the FIRST match in place and drops the rest, append() goes to the
// end, delete() takes an optional value — and the two-way live link between a
// URL's search string and its searchParams object.

const str = function (p: URLSearchParams): string {
  return p.toString();
};

// set() on an existing name keeps the position of the first occurrence.
const s = new URLSearchParams("a=1&b=2&a=3&c=4&a=5");
console.log("start=" + str(s));
s.set("a", "9");
console.log("set_existing=" + str(s) + " size=" + s.size);
s.set("z", "0");
console.log("set_new=" + str(s));
s.append("a", "10");
console.log("append=" + str(s));
console.log("get_first=" + s.get("a") + " all=" + s.getAll("a").join("|"));
s.set("a", "11");
console.log("set_collapses=" + str(s));

// delete() removes every match unless a value is given.
const d = new URLSearchParams("a=1&b=2&a=2&a=1");
d.delete("a", "1");
console.log("delete_value=" + str(d));
d.delete("a");
console.log("delete_name=" + str(d));
d.delete("missing");
console.log("delete_missing=" + str(d) + " size=" + d.size);
const dEmpty = new URLSearchParams("=x&a=1");
dEmpty.delete("");
console.log("delete_empty_name=" + str(dEmpty));

// has() takes the same optional value argument.
const h = new URLSearchParams("a=1&a=2&b=");
console.log("has=" + h.has("a") + "," + h.has("z"));
console.log("has_value=" + h.has("a", "2") + "," + h.has("a", "3"));
console.log("has_empty_value=" + h.has("b", "") + "," + h.has("b", "x"));
console.log("get_missing=" + String(h.get("z")) + " getAll_missing=" + JSON.stringify(h.getAll("z")));

// sort() is by code unit on the NAME only, and is stable for equal names.
const so = new URLSearchParams("b=1&a=3&B=0&a=1&A=2&b=0&a=2");
so.sort();
console.log("sort=" + str(so));
const stable = new URLSearchParams("x=first&x=second&x=third&a=1");
stable.sort();
console.log("sort_stable=" + str(stable));
const units = new URLSearchParams("é=1&z=2&Z=3&\u{1F600}=4&a=5");
units.sort();
console.log("sort_code_units=" + str(units));
console.log("sort_returns=" + String(new URLSearchParams("b=1&a=2").sort()));

// Iterating reflects the current order, and mutation during iteration is seen.
const live = new URLSearchParams("a=1&b=2");
const seen: string[] = [];
live.forEach(function (value, key, parent) {
  seen.push(key + "=" + value);
  if (key === "a") parent.append("c", "3");
});
console.log("forEach_sees_appends=" + seen.join(","));
console.log("forEach_third_arg_is_self=" + (function (): boolean {
  let ok = false;
  live.forEach(function (_v, _k, parent) {
    ok = parent === live;
  });
  return ok;
})());

// The live link: url.search and url.searchParams are two views of one list.
const u = new URL("https://example.com/p?a=1&b=2");
const params = u.searchParams;
console.log("same_object_each_read=" + (u.searchParams === params));
params.append("c", "3");
console.log("append_updates_search=" + u.search);
console.log("append_updates_href=" + u.href);
params.set("a", "9");
console.log("set_updates_search=" + u.search);
params.delete("b");
console.log("delete_updates_search=" + u.search);
u.search = "z=100&y=200";
console.log("search_updates_params=" + str(params) + " still_same=" + (u.searchParams === params));
u.search = "";
console.log("cleared_params=" + JSON.stringify(str(params)) + " href=" + u.href);
params.append("q", "a b");
console.log("reappend=" + u.search + " href=" + u.href);
params.delete("q");
console.log("emptied=" + JSON.stringify(u.search) + " href=" + u.href);

// href assignment replaces the list under the same object.
u.href = "https://example.com/x?k=v";
console.log("href_updates_params=" + str(params) + " same=" + (u.searchParams === params));

// A detached URLSearchParams has no URL to update.
const detached = new URLSearchParams("a=1");
detached.append("b", "2");
console.log("detached=" + str(detached) + " unrelated_url=" + u.search);

// Sorting through the live object rewrites the URL.
u.search = "b=2&a=1&c=3";
params.sort();
console.log("sort_rewrites_href=" + u.href);

// A value containing the separators is re-encoded when written back.
params.set("k", "x&y=z#w");
console.log("encoded_back=" + u.search);
console.log("decoded_again=" + JSON.stringify(u.searchParams.get("k")));
console.log("hash_untouched=" + JSON.stringify(u.hash));
