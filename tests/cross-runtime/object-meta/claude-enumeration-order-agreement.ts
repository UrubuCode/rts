// Pins that every own-key enumeration reaches the SAME order from the same
// object — Object.keys, for-in, JSON.stringify, Object.assign, spread,
// entries, fromEntries round-trip — and that deleting then re-adding a key
// moves it to the END of the string group while an index keeps its slot.

const o: any = {};
o["b"] = 1;
o["3"] = 2;
o["a"] = 3;
o["1"] = 4;
o["c"] = 5;
o["2"] = 6;

function forinOf(x: any): string {
  const out: string[] = [];
  for (const k in x) out.push(k);
  return out.join("|");
}
function jsonKeys(x: any): string {
  const parsed = JSON.parse(JSON.stringify(x));
  return Object.keys(parsed).join("|");
}

const canonical = Object.keys(o).join("|");
console.log("keys=" + canonical);
console.log("forin_same=" + (forinOf(o) === canonical));
console.log("json_same=" + (jsonKeys(o) === canonical));
console.log("assign_same=" + (Object.keys(Object.assign({}, o)).join("|") === canonical));
console.log("spread_same=" + (Object.keys({ ...o }).join("|") === canonical));
console.log("entries_same=" + (Object.entries(o).map((e) => e[0]).join("|") === canonical));
console.log("ownnames_same=" + (Object.getOwnPropertyNames(o).join("|") === canonical));
console.log("reflect_same=" + (Reflect.ownKeys(o).join("|") === canonical));
console.log("descs_same=" + (Object.keys(Object.getOwnPropertyDescriptors(o)).join("|") === canonical));
console.log("fromentries_same=" + (Object.keys(Object.fromEntries(Object.entries(o))).join("|") === canonical));

// deleting and re-adding a STRING key moves it to the end
delete o["a"];
o["a"] = 9;
console.log("readd_string=" + Object.keys(o).join("|"));
// an INDEX key keeps its numeric slot no matter when it was added
delete o["1"];
o["1"] = 8;
console.log("readd_index=" + Object.keys(o).join("|"));
// overwriting a value never moves the key
o["b"] = 100;
console.log("overwrite=" + Object.keys(o).join("|"));

// defineProperty places a NEW key at the end of its group, like assignment
const d: any = { x: 1, z: 2 };
Object.defineProperty(d, "y", { value: 3, enumerable: true, configurable: true, writable: true });
console.log("define_order=" + Object.keys(d).join("|"));
// redefining an existing one does not move it
Object.defineProperty(d, "x", { value: 10 });
console.log("redefine_order=" + Object.keys(d).join("|"));

// a non-enumerable key holds its slot in ownKeys and is absent from keys
const m: any = { p: 1, q: 2, r: 3 };
Object.defineProperty(m, "q", { enumerable: false });
console.log("mixed_own=" + Object.getOwnPropertyNames(m).join("|"));
console.log("mixed_keys=" + Object.keys(m).join("|"));
console.log("mixed_forin=" + forinOf(m));
console.log("mixed_json=" + JSON.stringify(m));

// symbols come after every string key, in insertion order, in ownKeys only
const s1 = Symbol("one");
const s2 = Symbol("two");
const withSyms: any = {};
withSyms[s2] = 1;
withSyms["late"] = 2;
withSyms[s1] = 3;
withSyms["9"] = 4;
console.log("sym_own=" + Reflect.ownKeys(withSyms).map(String).join("|"));
console.log("sym_names=" + Object.getOwnPropertyNames(withSyms).join("|"));
console.log("sym_symbols=" + Object.getOwnPropertySymbols(withSyms).map(String).join("|"));

// class instances follow the same rule with fields assigned in constructor order
class Rec {
  constructor() {
    (this as any).zeta = 1;
    (this as any)["7"] = 2;
    (this as any).alpha = 3;
    (this as any)["2"] = 4;
  }
}
console.log("class_keys=" + Object.keys(new Rec()).join("|"));

// an object built from a Map preserves the Map's insertion order for strings
const map = new Map<string, number>([["m", 1], ["5", 2], ["k", 3], ["0", 4]]);
console.log("frommap=" + Object.keys(Object.fromEntries(map)).join("|"));
console.log("map_iter=" + Array.from(map.keys()).join("|"));

// a null-prototype object orders identically
const bare: any = Object.create(null);
bare["b"] = 1;
bare["3"] = 2;
bare["a"] = 3;
bare["1"] = 4;
console.log("bare_keys=" + Object.keys(bare).join("|"));
console.log("bare_forin=" + forinOf(bare));

// JSON.stringify with an explicit key list uses the LIST's order, not the object's
console.log("json_list=" + JSON.stringify(o, ["c", "b", "3"]));
