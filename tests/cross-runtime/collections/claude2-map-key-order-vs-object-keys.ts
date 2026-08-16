// Cross-runtime: a Map keeps INSERTION order for every key; a plain object
// sorts its integer-like keys ahead of the rest and coerces every key to a
// string. Converting between the two is therefore not a round trip.

const insert = ["2", "10", "1", "b", "a", "0"];

// --- the object reorders; the Map does not ---
const obj: any = {};
for (const k of insert) obj[k] = k;
console.log("object_keys=" + Object.keys(obj).join(","));
console.log("object_names=" + Object.getOwnPropertyNames(obj).join(","));
let forIn = "";
for (const k in obj) forIn += k + ",";
console.log("object_for_in=" + forIn);

const map = new Map<any, string>();
for (const k of insert) map.set(k, k);
console.log("map_keys=" + [...map.keys()].join(","));
console.log("map_json=" + JSON.stringify([...map.keys()]));

// --- what counts as an integer index for an object: canonical, unsigned ---
const edge: any = {};
edge["4294967296"] = 1;
edge["-1"] = 1;
edge["01"] = 1;
edge["1.0"] = 1;
edge["1e2"] = 1;
edge["9007199254740992"] = 1;
edge["3"] = 1;
console.log("object_edge_keys=" + Object.keys(edge).join(","));

const edgeMap = new Map<any, number>();
for (const k of ["4294967296", "-1", "01", "1.0", "1e2", "9007199254740992", "3"]) edgeMap.set(k, 1);
console.log("map_edge_keys=" + [...edgeMap.keys()].join(","));

// --- an object coerces the key; a Map does not ---
const coerce: any = {};
coerce[1] = "number";
coerce["1"] = "string";
coerce[true] = "boolean";
coerce["true"] = "boolean_string";
console.log("object_coerced_count=" + Object.keys(coerce).length);
console.log("object_one=" + coerce[1] + ":" + coerce["1"]);

const distinct = new Map<any, string>();
distinct.set(1, "number");
distinct.set("1", "string");
distinct.set(true, "boolean");
distinct.set("true", "boolean_string");
console.log("map_distinct_count=" + distinct.size);
console.log("map_one=" + distinct.get(1) + ":" + distinct.get("1"));
console.log("map_true=" + distinct.get(true) + ":" + distinct.get("true"));

// --- an object key of -0 becomes "0"; a Map key of -0 becomes +0 ---
const zeroObj: any = {};
zeroObj[-0] = "obj";
console.log("object_neg_zero_key=" + JSON.stringify(Object.keys(zeroObj)));
const zeroMap = new Map<any, string>([[-0, "map"]]);
console.log("map_neg_zero_key=" + (1 / ([...zeroMap.keys()][0] as number)));
console.log("map_neg_zero_get=" + zeroMap.get(0) + ":" + zeroMap.get(-0));

// --- a symbol key sits last on an object and in place in a Map ---
const sym = Symbol("s");
const mixed: any = {};
mixed[sym] = 1;
mixed["z"] = 2;
mixed["1"] = 3;
console.log("object_ownkeys=" + Reflect.ownKeys(mixed).map(String).join(","));

const mixedMap = new Map<any, number>();
mixedMap.set(sym, 1);
mixedMap.set("z", 2);
mixedMap.set("1", 3);
console.log("map_ownkeys=" + [...mixedMap.keys()].map(String).join(","));

// --- so the two conversions lose different things ---
const fromMap = Object.fromEntries(map);
console.log("fromEntries_keys=" + Object.keys(fromMap).join(","));
console.log("fromEntries_reordered=" + (Object.keys(fromMap).join(",") !== [...map.keys()].join(",")));

const backAgain = new Map(Object.entries(fromMap));
console.log("round_trip_keys=" + [...backAgain.keys()].join(","));
console.log("round_trip_equal=" + ([...backAgain.keys()].join(",") === [...map.keys()].join(",")));

const numericMap = new Map<any, string>([[2, "two"], [1, "one"]]);
const numericObj = Object.fromEntries(numericMap);
console.log("numeric_keys_become_strings=" + JSON.stringify(Object.keys(numericObj)));
const numericBack = new Map(Object.entries(numericObj));
console.log("numeric_back_types=" + typeof [...numericBack.keys()][0]);
console.log("numeric_back_get=" + String(numericBack.get(1)) + ":" + numericBack.get("1"));

// --- deleting and re-adding moves a Map key to the end, but not an object key ---
const reorder = new Map([["a", 1], ["b", 2], ["c", 3]]);
reorder.delete("a");
reorder.set("a", 9);
console.log("map_readd=" + [...reorder.keys()].join(","));

const objReorder: any = { a: 1, b: 2, c: 3 };
delete objReorder.a;
objReorder.a = 9;
console.log("object_readd=" + Object.keys(objReorder).join(","));

// --- an object's integer keys sort ahead even when added last ---
const late: any = { z: 1, y: 2 };
late[5] = 3;
console.log("object_late_integer=" + Object.keys(late).join(","));
const lateMap = new Map<any, number>([["z", 1], ["y", 2]]);
lateMap.set(5, 3);
console.log("map_late_integer=" + [...lateMap.keys()].join(","));

// --- Object.groupBy sorts its integer-like keys, Map.groupBy does not ---
const items = [3, 1, 2];
const grouped: any = Object.groupBy(items, (n: number) => String(n * 10));
console.log("object_groupBy_keys=" + Object.keys(grouped).join(","));
const groupedMap = Map.groupBy(items, (n: number) => String(n * 10));
console.log("map_groupBy_keys=" + [...groupedMap.keys()].join(","));
