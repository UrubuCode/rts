// Pins the for-in "already visited" rule: a name is reported at most once, and
// a NON-ENUMERABLE own property hides an enumerable one of the same name higher
// up the chain — the property is skipped entirely, not reported from the proto.
// 419_property_enumeration_mutation covers mutation during the loop, not this.

const grand: any = { a: "grand-a", b: "grand-b", c: "grand-c", d: "grand-d" };
const mid: any = Object.create(grand);
mid.b = "mid-b";
Object.defineProperty(mid, "c", { value: "mid-c", enumerable: false, configurable: true, writable: true });
const leaf: any = Object.create(mid);
leaf.a = "leaf-a";

const seen: string[] = [];
for (const k in leaf) seen.push(k + ":" + leaf[k]);
console.log("forin=" + seen.join("|"));
console.log("c_readable=" + leaf.c);
console.log("c_in=" + ("c" in leaf));
console.log("d_seen=" + (seen.join("|").indexOf("d:") >= 0));

// the shadowing non-enumerable is on `mid`, so enumerating `mid` also skips it
const midSeen: string[] = [];
for (const k in mid) midSeen.push(k);
console.log("forin_mid=" + midSeen.join("|"));

// making it enumerable again brings the name back, with the SHADOWING value
Object.defineProperty(mid, "c", { enumerable: true });
const seen2: string[] = [];
for (const k in leaf) seen2.push(k + ":" + leaf[k]);
console.log("forin_after=" + seen2.join("|"));

// Object.keys is own-only and ignores the chain entirely
console.log("keys_leaf=" + Object.keys(leaf).join("|"));
console.log("keys_mid=" + Object.keys(mid).join("|"));

// for-in over an object whose prototype is null
const bare: any = Object.create(null);
bare.x = 1;
bare.y = 2;
const bareSeen: string[] = [];
for (const k in bare) bareSeen.push(k);
console.log("forin_bare=" + bareSeen.join("|"));

// built-in prototypes are non-enumerable, so a plain object yields own keys only
const plain: any = { p: 1 };
const plainSeen: string[] = [];
for (const k in plain) plainSeen.push(k);
console.log("forin_plain=" + plainSeen.join("|"));

// an ACCESSOR on the prototype is enumerated by name and read through the getter
const accProto: any = {};
Object.defineProperty(accProto, "acc", { get() { return "from-getter"; }, enumerable: true, configurable: true });
const accChild: any = Object.create(accProto);
accChild.own = "o";
const accSeen: string[] = [];
for (const k in accChild) accSeen.push(k + "=" + accChild[k]);
console.log("forin_acc=" + accSeen.join("|"));

// symbols never appear in for-in
const s = Symbol("hidden");
const withSym: any = { visible: 1 };
withSym[s] = 2;
const symSeen: string[] = [];
for (const k in withSym) symSeen.push(String(k));
console.log("forin_sym=" + symSeen.join("|"));
console.log("ownkeys_sym=" + Reflect.ownKeys(withSym).map(String).join("|"));

// for-in over an array visits index keys as STRINGS, then own string keys
const arr: any = [10, 20];
arr.extra = 30;
const arrSeen: string[] = [];
for (const k in arr) arrSeen.push(typeof k + ":" + k);
console.log("forin_arr=" + arrSeen.join("|"));

// a hole is not visited
const holed: any = [1, , 3];
const holeSeen: string[] = [];
for (const k in holed) holeSeen.push(k);
console.log("forin_hole=" + holeSeen.join("|"));

// a for-in over a string visits its index keys, not its methods
const strSeen: string[] = [];
for (const k in "ab") strSeen.push(k);
console.log("forin_string=" + strSeen.join("|"));

// for-in over a primitive with no own keys yields nothing, and null/undefined
// are simply skipped rather than throwing
const numSeen: string[] = [];
for (const k in 7 as any) numSeen.push(k);
console.log("forin_number=" + numSeen.length);
const nullSeen: string[] = [];
for (const k in null as any) nullSeen.push(k);
for (const k in undefined as any) nullSeen.push(k);
console.log("forin_nullish=" + nullSeen.length);

// the loop VARIABLE is always a string, even over integer-like keys
const typeSeen: string[] = [];
for (const k in { 1: "a", x: "b" } as any) typeSeen.push(typeof k);
console.log("forin_keytype=" + typeSeen.join("|"));

// making an inherited name non-enumerable on the PROTOTYPE hides it everywhere
const hideProto: any = { visible: 1, gone: 2 };
Object.defineProperty(hideProto, "gone", { enumerable: false });
const hideChild: any = Object.create(hideProto);
const hideSeen: string[] = [];
for (const k in hideChild) hideSeen.push(k);
console.log("forin_hidden_proto=" + hideSeen.join("|") + ",read=" + hideChild.gone);

// a name shadowed by an ENUMERABLE own property is reported once, from the child
const dupProto: any = { dup: "P" };
const dupChild: any = Object.create(dupProto);
dupChild.dup = "C";
const dupSeen: string[] = [];
for (const k in dupChild) dupSeen.push(k + "=" + dupChild[k]);
console.log("forin_dup=" + dupSeen.join("|") + ",count=" + dupSeen.length);

// setting the prototype to null mid-loop stops the walk after the own keys
const cutProto: any = { pa: 1, pb: 2 };
const cut: any = Object.create(cutProto);
cut.own = 0;
const cutSeen: string[] = [];
for (const k in cut) {
  cutSeen.push(k);
  if (k === "own") Object.setPrototypeOf(cut, null);
}
console.log("forin_cut=" + cutSeen.join("|"));
