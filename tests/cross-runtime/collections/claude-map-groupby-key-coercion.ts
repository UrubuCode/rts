// Cross-runtime: Map.groupBy vs Object.groupBy KEY handling — Map.groupBy keeps
// the key as-is (SameValueZero, objects allowed); Object.groupBy coerces to
// property key (string/symbol). One thing: what the group key becomes.

const items = [1, 2, 3, 4, 5];

// --- Object.groupBy coerces the callback result to a string ---
const og = Object.groupBy(items, (x) => x % 2 === 0 ? "even" : "odd");
console.log("obj_keys=" + Object.keys(og).join(","));
console.log("obj_even=" + og.even!.join(","));
console.log("obj_odd=" + og.odd!.join(","));

// numeric keys become strings on the object
const og2: any = Object.groupBy(items, (x) => x % 2);
console.log("obj_num_keys=" + Object.keys(og2).join(","));
console.log("obj_key_types=" + Object.keys(og2).map(k => typeof k).join(","));
console.log("obj_get_0=" + og2[0].join(",") + ":" + og2["0"].join(","));

// --- Object.groupBy result has a null prototype ---
console.log("obj_proto_null=" + (Object.getPrototypeOf(og) === null));
console.log("obj_no_tostring=" + (typeof (og as any).toString));

// --- Map.groupBy keeps the raw key and its type ---
const mg = Map.groupBy(items, (x) => x % 2);
console.log("map_keys=" + [...mg.keys()].join(","));
console.log("map_key_types=" + [...mg.keys()].map(k => typeof k).join(","));
console.log("map_get_num=" + mg.get(0)!.join(","));
console.log("map_get_str_miss=" + String(mg.get("0" as any)));

// --- Map.groupBy with OBJECT keys (impossible on Object.groupBy) ---
const gA = { name: "A" };
const gB = { name: "B" };
const mg2 = Map.groupBy(items, (x) => x > 3 ? gA : gB);
console.log("map_obj_size=" + mg2.size);
console.log("map_obj_A=" + mg2.get(gA)!.join(","));
console.log("map_obj_B=" + mg2.get(gB)!.join(","));
console.log("map_obj_key_names=" + [...mg2.keys()].map((k: any) => k.name).join(","));
console.log("map_obj_fresh_key_miss=" + String(mg2.get({ name: "A" })));

// --- Map.groupBy SameValueZero: -0 folds into +0, NaN groups together ---
const mg3 = Map.groupBy([1, 2, 3], (x) => x === 1 ? -0 : (x === 2 ? 0 : NaN));
console.log("svz_size=" + mg3.size);
console.log("svz_zero=" + mg3.get(0)!.join(","));
console.log("svz_key_1_div=" + (1 / [...mg3.keys()][0]));
console.log("svz_nan=" + mg3.get(NaN)!.join(","));

// --- Object.groupBy: -0/NaN/undefined/null keys stringify ---
const og3: any = Object.groupBy([1, 2, 3, 4], (x) =>
  x === 1 ? -0 : x === 2 ? NaN : x === 3 ? undefined : null);
console.log("obj_coerced_keys=" + Object.keys(og3).join("|"));

// --- callback gets (element, index) ---
const idx: string[] = [];
Map.groupBy(["a", "b"], (v, i) => { idx.push(v + "@" + i); return v; });
console.log("cb_args=" + idx.join(","));
const oidx: string[] = [];
Object.groupBy(["a", "b"], (v, i) => { oidx.push(v + "@" + i); return v; });
console.log("obj_cb_args=" + oidx.join(","));

// --- group order follows first-appearance ---
const mg4 = Map.groupBy([3, 1, 3, 2, 1], (x) => x);
console.log("order=" + [...mg4.keys()].join(","));
console.log("groups=" + [...mg4.values()].map(v => v.join("+")).join("|"));

// --- boolean keys ---
const mg5 = Map.groupBy(items, (x) => x > 2);
console.log("bool_keys=" + [...mg5.keys()].join(",") + ":types=" + [...mg5.keys()].map(k => typeof k).join(","));
console.log("bool_true=" + mg5.get(true)!.join(","));

// --- empty input ---
console.log("empty_map=" + Map.groupBy([], (x: any) => x).size);
console.log("empty_obj=" + Object.keys(Object.groupBy([], (x: any) => x)).length);

// --- groupBy accepts any iterable, values are plain arrays ---
const mg6 = Map.groupBy(new Set([1, 2, 3, 4]), (x) => x % 2);
console.log("from_set=" + [...mg6.keys()].join(",") + ":odd=" + mg6.get(1)!.join(","));
console.log("values_are_arrays=" + Array.isArray(mg6.get(1)));
