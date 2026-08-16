// Cross-runtime: the reviver's walk over an ARRAY. The keys it sees are index
// STRINGS, `length` is never among them, `this` is the array itself, and a
// reviver returning undefined leaves a genuine HOLE rather than an undefined
// element.

// --- the visit sequence: children before parents, indices as strings ---
const order: string[] = [];
const revived: any = JSON.parse('[10,[20,21],{"k":30}]', function (this: any, key: string, value: any) {
  order.push(key + "(" + typeof key + ")" + ":" + (Array.isArray(this) ? "arr" : typeof this));
  return value;
});
console.log("order=" + order.join("|"));
console.log("result=" + JSON.stringify(revived));

// --- length is never visited, and the array is a real Array by then ---
const shapes: string[] = [];
JSON.parse('[1,2]', function (this: any, key: string, value: any) {
  shapes.push(key + ":" + (Array.isArray(this) ? "isArray" : "notArray") + ":len=" + (this as any).length);
  return value;
});
console.log("holder_shapes=" + shapes.join("|"));

// --- returning undefined deletes the element and leaves a hole ---
const holed: any = JSON.parse('[1,2,3]', function (key: string, value: any) {
  return key === "1" ? undefined : value;
});
console.log("holed_length=" + holed.length);
console.log("holed_json=" + JSON.stringify(holed));
console.log("holed_has_index=" + (1 in holed) + ":" + Object.prototype.hasOwnProperty.call(holed, "1"));
console.log("holed_read=" + String(holed[1]));
console.log("holed_keys=" + Object.keys(holed).join(","));
let visitedHole = "";
holed.forEach((v: any, i: number) => { visitedHole += i + ":" + v + ","; });
console.log("holed_forEach=" + visitedHole);
console.log("holed_join=" + holed.join("-"));
console.log("holed_spread=" + [...holed].map(String).join(","));

// --- the same reviver on an OBJECT deletes the key outright ---
const objHoled: any = JSON.parse('{"a":1,"b":2}', function (key: string, value: any) {
  return key === "b" ? undefined : value;
});
console.log("object_keys=" + Object.keys(objHoled).join(","));
console.log("object_has_b=" + ("b" in objHoled));

// --- replacing an element with a different type ---
const doubled: any = JSON.parse('[1,2,3]', function (key: string, value: any) {
  return key === "" ? value : (typeof value === "number" ? value * 2 : value);
});
console.log("doubled=" + JSON.stringify(doubled));

// --- the root key is the empty string and its holder is a fresh wrapper ---
let rootInfo = "";
JSON.parse('[1]', function (this: any, key: string, value: any) {
  if (key === "") {
    rootInfo = "key=" + JSON.stringify(key) +
      ",holder_is_array=" + Array.isArray(this) +
      ",holder_keys=" + Object.keys(this).map((k) => JSON.stringify(k)).join("/") +
      ",value_is_array=" + Array.isArray(value);
  }
  return value;
});
console.log("root=" + rootInfo);

// --- the reviver may replace the root entirely ---
console.log("root_replaced=" + JSON.parse('[1,2]', (k, v) => (k === "" ? "ROOT" : v)));
console.log("root_deleted=" + String(JSON.parse('[1,2]', (k, v) => (k === "" ? undefined : v))));

// --- mutating the holder mid-walk: a later sibling read reflects the change ---
const seenValues: string[] = [];
const mutated: any = JSON.parse('[1,2,3]', function (this: any, key: string, value: any) {
  if (key !== "") seenValues.push(key + "=" + value);
  if (key === "0") (this as any)[2] = 99;
  return value;
});
console.log("mutation_seen=" + seenValues.join(","));
console.log("mutation_result=" + JSON.stringify(mutated));

// --- growing the array mid-walk: the extra index is NOT visited ---
const grownKeys: string[] = [];
const grown: any = JSON.parse('[1,2]', function (this: any, key: string, value: any) {
  if (key !== "") grownKeys.push(key);
  if (key === "0" && Array.isArray(this)) (this as any).push("added");
  return value;
});
console.log("grown_keys=" + grownKeys.join(","));
console.log("grown_result=" + JSON.stringify(grown));

// --- deleting a not-yet-visited sibling: it is still visited, as undefined ---
const deletedKeys: string[] = [];
const deleted: any = JSON.parse('[1,2,3]', function (this: any, key: string, value: any) {
  if (key !== "") deletedKeys.push(key + "=" + String(value));
  if (key === "0") delete (this as any)[2];
  return value;
});
console.log("deleted_keys=" + deletedKeys.join(","));
console.log("deleted_result=" + JSON.stringify(deleted));

// --- nesting: an inner array is complete before its parent's turn ---
const nestedTrace: string[] = [];
JSON.parse('{"outer":[1,{"inner":2}]}', function (this: any, key: string, value: any) {
  nestedTrace.push(key + "=" + (typeof value === "object" && value !== null ? (Array.isArray(value) ? "[" + value.length + "]" : "{}") : String(value)));
  return value;
});
console.log("nested_trace=" + nestedTrace.join("|"));

// --- the arrays a reviver receives are ordinary and mutable afterwards ---
const plainArr: any = JSON.parse('[1,2]', (k, v) => v);
console.log("proto_is_array=" + (Object.getPrototypeOf(plainArr) === Array.prototype));
console.log("extensible=" + Object.isExtensible(plainArr));
plainArr.push(3);
console.log("after_push=" + JSON.stringify(plainArr));

// --- a non-callable reviver is ignored, not refused ---
console.log("reviver_number=" + JSON.stringify(JSON.parse('[1,2]', 5 as any)));
console.log("reviver_null=" + JSON.stringify(JSON.parse('[1,2]', null as any)));
console.log("reviver_object=" + JSON.stringify(JSON.parse('[1,2]', {} as any)));
console.log("reviver_undefined=" + JSON.stringify(JSON.parse('[1,2]', undefined)));

// --- a reviver that throws stops the walk ---
try {
  JSON.parse('[1,2,3]', function (key: string) { if (key === "1") throw new RangeError("stop"); return 0; });
  console.log("throwing_reviver=no_throw");
} catch (e: any) {
  console.log("throwing_reviver=" + e.constructor.name);
}
