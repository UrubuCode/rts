// ONE thing: the 2^32-2 array-index boundary. Writing there must set length to
// 2^32-1 and store SPARSELY — an engine that materialises the slots cannot pass
// this file, which is why it is kept apart from the other index-key checks.
const a: any = [];
a[4294967294] = "last";
console.log("maxIndexLen=" + a.length);
console.log("maxIndexVal=" + a[4294967294]);
console.log("maxIndexKeys=" + Object.keys(a).join("|"));
console.log("maxIndexIn=" + (0 in a) + " " + (4294967294 in a));

// 2^32-1 is NOT an array index: it becomes an ordinary string key.
const b: any = [];
b[4294967295] = "beyond";
console.log("beyondLen=" + b.length);
console.log("beyondKeys=" + Object.keys(b).join("|"));
console.log("beyondVal=" + b[4294967295]);

// And so is anything above it.
const c: any = [];
c[4294967296] = "way-beyond";
console.log("aboveLen=" + c.length + " keys=" + Object.keys(c).join("|"));

// length may be set to exactly 2^32-1 but not beyond.
const d: any = [];
d.length = 4294967295;
console.log("setMaxLen=" + d.length + " keys=" + Object.keys(d).length);
try { d.length = 4294967296; } catch (e: any) { console.log("setBeyond=" + e.constructor.name); }
console.log("stillMax=" + d.length);

// new Array(n) accepts the same range.
console.log("ctorMax=" + new Array(4294967295).length);
try { new Array(4294967296); } catch (e: any) { console.log("ctorBeyond=" + e.constructor.name); }
try { new Array(-1); } catch (e: any) { console.log("ctorNeg=" + e.constructor.name); }
try { new Array(1.5); } catch (e: any) { console.log("ctorFrac=" + e.constructor.name); }

// A huge sparse array still answers its cheap queries without materialising.
const e: any = [];
e[4000000000] = "x";
// Only O(1) queries here on purpose: indexOf/join over a 4-billion-slot array
// walks every index and hangs Bun and Node too, so it would measure the
// harness rather than the engine.
console.log("hugeLen=" + e.length + " hugeKeys=" + Object.keys(e).length + " hugeLast=" + e.at(-1));
console.log("hugeHas=" + (4000000000 in e) + " " + (3999999999 in e));
