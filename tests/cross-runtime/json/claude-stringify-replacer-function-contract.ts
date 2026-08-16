// Cross-runtime: the JSON.stringify REPLACER function contract — it is called
// with `this` = the holder, the key always as a STRING (including "" for the
// root and "0","1",... for array indices), and it sees the value AFTER toJSON.

// --- the call log: key, typeof key, and whether `this` is the holder ---
const log: string[] = [];
const data: any = { a: 1, nested: { b: 2 }, list: [10, 20] };
const out = JSON.stringify(data, function (k: any, v: any) {
  log.push("[" + k + "|" + typeof k + "|" + (this === data ? "root" : Array.isArray(this) ? "arr" : "obj") + "]");
  return v;
});
console.log("visits=" + log.join(""));
console.log("out=" + out);
console.log("root_key_is_empty=" + (log[0] === "[|string|root]"));

// --- the root call receives the value wrapped in a fresh holder ---
let rootThisKeys = "";
let rootValue = "";
JSON.stringify(42, function (k: any, v: any) {
  rootThisKeys = Object.keys(this).join(",");
  rootValue = String(v) + ":" + typeof v;
  return v;
});
console.log("root_holder_keys=" + rootThisKeys);
console.log("root_value=" + rootValue);

// --- array indices arrive as strings ---
const idx: string[] = [];
JSON.stringify(["x", "y", "z"], function (k: any, v: any) {
  if (k !== "") idx.push(k + "/" + typeof k);
  return v;
});
console.log("array_keys=" + idx.join(","));

// --- returning undefined drops the key in an object, gives null in an array ---
console.log("drop_obj=" + JSON.stringify({ a: 1, b: 2 }, function (k: any, v: any) { return k === "b" ? undefined : v; }));
console.log("drop_arr=" + JSON.stringify([1, 2, 3], function (k: any, v: any) { return k === "1" ? undefined : v; }));
console.log("drop_root=" + String(JSON.stringify({ a: 1 }, function () { return undefined; })));

// --- the replacer sees the value toJSON already produced ---
const withToJSON: any = { toJSON() { return "FROM_TOJSON"; } };
console.log("after_tojson=" + JSON.stringify({ w: withToJSON }, function (k: any, v: any) {
  return k === "w" ? "saw:" + String(v) : v;
}));

// --- ... and it can replace a value with one that has its OWN toJSON ---
console.log("replacer_returns_tojson=" + JSON.stringify({ a: 1 }, function (k: any, v: any) {
  return k === "a" ? { toJSON() { return "inner"; } } : v;
}));

// --- the replacer result is walked recursively ---
console.log("recursive=" + JSON.stringify({ a: 1 }, function (k: any, v: any) {
  return k === "a" ? { deep: [1, 2] } : v;
}));

// --- replacing a primitive with an object, and vice versa ---
console.log("prim_to_obj=" + JSON.stringify(1, function (k: any, v: any) { return k === "" ? { n: v } : v; }));
console.log("obj_to_prim=" + JSON.stringify({ a: { b: 1 } }, function (k: any, v: any) { return k === "a" ? 0 : v; }));

// --- a replacer that is neither callable nor an array is IGNORED ---
console.log("replacer_number=" + JSON.stringify({ a: 1 }, 5 as any));
console.log("replacer_string=" + JSON.stringify({ a: 1 }, "a" as any));
console.log("replacer_null=" + JSON.stringify({ a: 1 }, null));
console.log("replacer_object=" + JSON.stringify({ a: 1 }, { a: true } as any));
console.log("replacer_true=" + JSON.stringify({ a: 1 }, true as any));

// --- a throwing replacer propagates ---
try {
  JSON.stringify({ a: 1 }, function (k: any) { if (k === "a") throw new RangeError("nope"); return 1; });
  console.log("throwing=no_throw");
} catch (e: any) { console.log("throwing=" + e.constructor.name); }

// --- the replacer also runs for values it cannot serialise ---
const kinds: string[] = [];
JSON.stringify({ u: undefined, f: function () { /* fn */ }, s: Symbol("x"), n: null }, function (k: any, v: any) {
  if (k !== "") kinds.push(k + ":" + typeof v);
  return v;
});
console.log("unserialisable_seen=" + kinds.join(","));

// --- and the replacer can rescue them ---
console.log("rescued=" + JSON.stringify({ u: undefined, f: function () { /* fn */ } }, function (k: any, v: any) {
  return k === "" ? v : typeof v === "undefined" ? "was-undefined" : typeof v === "function" ? "was-function" : v;
}));

// --- replacer + space together ---
console.log("with_space=" + JSON.stringify(JSON.stringify({ a: 1, b: 2 }, function (k: any, v: any) { return k === "b" ? 9 : v; }, 2)));

// --- shape of stringify itself ---
console.log("stringify_length=" + JSON.stringify.length + ":" + JSON.stringify.name);
console.log("json_tag=" + Object.prototype.toString.call(JSON));
