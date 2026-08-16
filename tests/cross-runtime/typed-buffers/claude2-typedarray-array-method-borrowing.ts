// Cross-runtime: a typed array is NOT an Array. Its own map/filter answer a typed
// array of the same kind, the Array mutators are absent entirely, and an
// Array.prototype method borrowed with .call treats it as a generic array-like.

const ta = new Uint8Array([3, 1, 2]);

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

console.log("is_array=" + Array.isArray(ta) + " instanceof=" + (ta instanceof Uint8Array) + "," + (ta instanceof Array));
console.log("own_map_kind=" + ta.map(function (x) { return x * 2; }).constructor.name + " values=" + ta.map(function (x) { return x * 2; }).join(","));
console.log("own_map_coerces=" + new Uint8Array([200]).map(function (x) { return x * 2; }).join(","));
console.log("own_filter_kind=" + ta.filter(function (x) { return x > 1; }).constructor.name + " values=" + ta.filter(function (x) { return x > 1; }).join(","));
console.log("own_slice_kind=" + ta.slice(1).constructor.name + " toReversed_kind=" + t(function () { return ta.toReversed().constructor.name; }));

// The mutators that would change the length simply do not exist.
console.log("absent=" + ["push", "pop", "shift", "unshift", "splice", "concat", "flat", "flatMap", "toSpliced"].map(function (k) { return k + ":" + typeof (ta as any)[k]; }).join(" "));
console.log("present=" + ["map", "filter", "reduce", "reduceRight", "forEach", "every", "some", "find", "findLast", "reverse", "sort", "join", "at", "with", "toSorted", "toReversed", "entries"].map(function (k) { return typeof (ta as any)[k]; }).join(","));

console.log("reduce=" + ta.reduce(function (a, b) { return a + b; }, 0) + " reduceRight=" + ta.reduceRight(function (a, b) { return a + "" + b; }, ""));
console.log("reduce_no_init=" + ta.reduce(function (a, b) { return a + b; }) + " empty_no_init=" + t(function () { return (new Uint8Array(0) as any).reduce(function (a: number, b: number) { return a + b; }); }));
console.log("every_some=" + ta.every(function (x) { return x > 0; }) + "," + ta.some(function (x) { return x > 2; }));
console.log("forEach_args=" + (function (): string {
  const seen: string[] = [];
  ta.forEach(function (v, i, arr) { seen.push(v + ":" + i + ":" + (arr === ta)); });
  return seen.join(" ");
})());
console.log("join=" + ta.join("-") + " join_default=" + ta.join() + " join_undefined=" + new Uint8Array([1, 2]).join(undefined));
console.log("string_conversion=" + String(ta) + " template=" + `${ta}` + " concat_string=" + ("x" + ta));
console.log("reverse_in_place=" + (function (): string {
  const a = new Uint8Array([1, 2, 3]);
  const r = a.reverse();
  return a.join(",") + "/" + String(r === a);
})());

// Borrowed Array.prototype methods see it as an array-like: they answer a plain
// Array and never a typed one.
console.log("borrow_map=" + t(function () { return Array.prototype.map.call(ta, function (x: number) { return x * 2; }).constructor.name; }));
console.log("borrow_filter=" + t(function () { return JSON.stringify(Array.prototype.filter.call(ta, function (x: number) { return x > 1; })); }));
console.log("borrow_slice=" + t(function () { return JSON.stringify(Array.prototype.slice.call(ta, 1)); }));
console.log("borrow_concat=" + t(function () { return JSON.stringify(Array.prototype.concat.call([], ta)); }));
console.log("borrow_join=" + t(function () { return Array.prototype.join.call(ta, "|"); }));
console.log("borrow_indexOf=" + t(function () { return Array.prototype.indexOf.call(ta, 1); }));
console.log("borrow_push=" + t(function () {
  const a = new Uint8Array([1, 2]);
  const r = Array.prototype.push.call(a as any, 9);
  return r + "/" + a.join(",") + "/" + String((a as any)[2]);
}));
console.log("borrow_sort_string_order=" + t(function () { return Array.prototype.sort.call(new Uint8Array([10, 9, 2])).join(","); }));

// And the reverse borrow: a typed method on a plain Array is refused.
console.log("typed_on_array=" + t(function () { return (Uint8Array.prototype.map as any).call([1, 2], function (x: number) { return x; }); }));
console.log("typed_join_on_array=" + t(function () { return (Uint8Array.prototype.join as any).call([1, 2], ","); }));
console.log("typed_slice_on_array=" + t(function () { return (Uint8Array.prototype.slice as any).call([1, 2], 0); }));
console.log("typed_map_on_wrong_kind=" + t(function () { return (Uint8Array.prototype.map as any).call(new Float64Array([1.5]), function (x: number) { return x; }).constructor.name; }));

// JSON sees index keys, because stringify walks own enumerable properties.
console.log("json=" + JSON.stringify(ta) + " json_empty=" + JSON.stringify(new Uint8Array(0)));
console.log("json_nested=" + JSON.stringify({ v: new Uint8Array([1]) }));
console.log("spread=" + JSON.stringify([...ta]) + " from=" + JSON.stringify(Array.from(ta)));
console.log("object_assign=" + JSON.stringify(Object.assign({}, new Uint8Array([7, 8]))));
console.log("keys=" + Object.keys(ta).join(",") + " values=" + Object.values(ta).join(","));
