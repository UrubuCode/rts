// Pins EnumerableOwnPropertyNames snapshotting the KEY list up front while
// reading values one by one: a getter that deletes a later key makes that key
// vanish from entries/values (skipped, not undefined), and one that ADDS a key
// leaves the new key out entirely.

function makeSubject(mutate: (self: any) => void): any {
  const o: any = {};
  Object.defineProperty(o, "a", { get() { return "A"; }, enumerable: true, configurable: true });
  Object.defineProperty(o, "b", { get() { mutate(o); return "B"; }, enumerable: true, configurable: true });
  Object.defineProperty(o, "c", { get() { return "C"; }, enumerable: true, configurable: true });
  Object.defineProperty(o, "d", { get() { return "D"; }, enumerable: true, configurable: true });
  return o;
}

// deleting a LATER key: it is skipped by the value-reading operations
const del = makeSubject((self) => { delete self.c; });
console.log("del_entries=" + Object.entries(del).map((e) => e[0] + ":" + e[1]).join("|"));
const del2 = makeSubject((self) => { delete self.c; });
console.log("del_values=" + Object.values(del2).join("|"));
const del3 = makeSubject((self) => { delete self.c; });
console.log("del_keys=" + Object.keys(del3).join("|"));
const del4 = makeSubject((self) => { delete self.c; });
console.log("del_json=" + JSON.stringify(del4));
const del5 = makeSubject((self) => { delete self.c; });
console.log("del_assign=" + JSON.stringify(Object.assign({}, del5)));
const del6 = makeSubject((self) => { delete self.c; });
console.log("del_spread=" + JSON.stringify({ ...del6 }));

// making a later key NON-ENUMERABLE also drops it from the result
const hide = makeSubject((self) => { Object.defineProperty(self, "c", { enumerable: false }); });
console.log("hide_entries=" + Object.entries(hide).map((e) => e[0]).join("|"));

// ADDING a key mid-read never appears: the key list was taken before the reads
const add = makeSubject((self) => { self.zz = "NEW"; });
console.log("add_entries=" + Object.entries(add).map((e) => e[0]).join("|"));
console.log("add_after=" + Object.keys(add).join("|"));

// deleting an EARLIER key changes nothing, it was already read
const back = makeSubject((self) => { delete self.a; });
console.log("back_entries=" + Object.entries(back).map((e) => e[0] + ":" + e[1]).join("|"));

// for-in walks live: a key deleted before it is reached is not visited
const live: any = { a: 1, b: 2, c: 3, d: 4 };
const visited: string[] = [];
for (const k in live) {
  visited.push(k);
  if (k === "b") delete live.c;
}
console.log("forin_live=" + visited.join("|"));

// Object.keys on a string yields the index strings, values yields the chars
console.log("str_keys=" + Object.keys("hey" as any).join("|"));
console.log("str_values=" + Object.values("hey" as any).join("|"));
console.log("str_entries=" + Object.entries("hey" as any).map((e) => e[0] + ":" + e[1]).join("|"));
console.log("str_empty=" + Object.keys("" as any).length);

// on a non-string primitive there are no own enumerable keys
console.log("num_keys=" + Object.keys(7 as any).length);
console.log("bool_values=" + Object.values(true as any).length);
try {
  console.log("null_keys=" + Object.keys(null as any).length);
} catch (e: any) {
  console.log("null_keys=throw:" + e.constructor.name);
}

// an array-like plain object enumerates its index keys ascending
const arrayLike: any = { length: 3, 2: "two", 0: "zero", 1: "one" };
console.log("arraylike=" + Object.entries(arrayLike).map((e) => e[0] + ":" + e[1]).join("|"));

// a sparse array skips its holes in all three
const sparse: any = [1, , 3];
sparse.tail = "T";
console.log("sparse_keys=" + Object.keys(sparse).join("|"));
console.log("sparse_values=" + Object.values(sparse).join("|"));

// a getter that throws aborts entries with nothing returned
const boom: any = {};
Object.defineProperty(boom, "ok", { get() { return 1; }, enumerable: true });
Object.defineProperty(boom, "bad", { get(): number { throw new RangeError("b"); }, enumerable: true });
try {
  console.log("boom=" + Object.entries(boom).length);
} catch (e: any) {
  console.log("boom=throw:" + e.constructor.name);
}
console.log("boom_keys_ok=" + Object.keys(boom).join("|"));

// Object.values reads through the prototype's accessor only if the property is OWN
const proto: any = {};
Object.defineProperty(proto, "inh", { get() { return "I"; }, enumerable: true, configurable: true });
const child: any = Object.create(proto);
child.own = "O";
console.log("child_values=" + Object.values(child).join("|"));
console.log("child_forin=" + (() => { const out: string[] = []; for (const k in child) out.push(k); return out.join("|"); })());
