// A read site that keeps missing remembers the refusal — so the thing to pin is
// that remembering it never makes the site answer for a property that has since
// appeared. Every case below arms the negative first (a warm loop), then makes
// the property exist, and asserts the site sees it.

function assert(ok: boolean, what: string): void {
  if (!ok) throw new Error("failed: " + what);
}

// --- the shape the regression had: two links away, so the site refuses ------
class Base {
  bp(): number {
    return 1;
  }
}
class Derived extends Base {
  x: number = 1;
}
const derived = new Derived();
let sum = 0;
for (let i = 0; i < 200; i++) sum += derived.bp();
assert(sum === 200, "a twice-inherited method answers while the site refuses");

// --- the link gains the key after the negative is armed --------------------
class Holder {
  own: number = 1;
}
const holder: any = new Holder();
let missing = 0;
for (let i = 0; i < 200; i++) {
  if (holder.later === undefined) missing = missing + 1;
}
assert(missing === 200, "an absent property reads undefined every time");
Holder.prototype.later = 7;
let seen = 0;
for (let i = 0; i < 200; i++) seen += holder.later;
assert(seen === 1400, "the property appearing ON THE LINK is seen after");

// --- delete, then add it back ----------------------------------------------
const plain: any = { a: 1 };
let got = 0;
for (let i = 0; i < 200; i++) got += plain.a;
assert(got === 200, "an own property reads while warm");
delete plain.a;
let gone = 0;
for (let i = 0; i < 200; i++) {
  if (plain.a === undefined) gone = gone + 1;
}
assert(gone === 200, "a deleted property reads undefined every time");
plain.a = 5;
let back = 0;
for (let i = 0; i < 200; i++) back += plain.a;
assert(back === 1000, "the property added back is seen again");

// --- the prototype reassigned under a warm site ----------------------------
const swapped: any = {};
Object.setPrototypeOf(swapped, { m: () => 2 });
let first = 0;
for (let i = 0; i < 200; i++) first += swapped.m();
assert(first === 400, "a method on the assigned prototype answers");
Object.setPrototypeOf(swapped, { m: () => 3 });
let second = 0;
for (let i = 0; i < 200; i++) second += swapped.m();
assert(second === 600, "reassigning the prototype changes what the site reads");

console.log("ok");
