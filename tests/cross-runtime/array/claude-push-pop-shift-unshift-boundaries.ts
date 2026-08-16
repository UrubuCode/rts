// ONE thing: the four mutators at their boundaries — what they RETURN, what
// they do to length, and how they treat holes at the ends.
const empty: any[] = [];
console.log("popEmpty=" + String(empty.pop()) + " len=" + empty.length);
console.log("shiftEmpty=" + String(empty.shift()) + " len=" + empty.length);

console.log("pushNone=" + [1, 2].push() );
console.log("pushMany=" + [].push(1, 2, 3));
console.log("unshiftNone=" + [1, 2].unshift());
console.log("unshiftMany=" + [3].unshift(1, 2));

// unshift shifts every element up, and holes shift with them.
const h: any[] = [1, , 3];
h.unshift("a");
console.log("unshiftHoles=" + [0, 1, 2, 3].map((i) => (i in h ? "y" : "n")).join("") + " v=" + h.map(String).join(","));

// shift pulls the first element down and shortens.
const s: any[] = [, 2, , 4];
const first = s.shift();
console.log("shiftHole=" + String(first) + " len=" + s.length + " in=" + [0, 1, 2].map((i) => (i in s ? "y" : "n")).join(""));

// pop on a trailing hole answers undefined and still shortens.
const p: any[] = [1, , ];
console.log("popHole=" + String(p.pop()) + " len=" + p.length);

// push respects a length that is a plain data property on an array-like.
const like: any = { length: 2, 0: "a", 1: "b" };
console.log("genericPush=" + Array.prototype.push.call(like, "c") + " len=" + like.length + " v=" + like[2]);
console.log("genericPop=" + Array.prototype.pop.call(like) + " len=" + like.length + " has2=" + (2 in like));

// A missing length starts at 0.
const noLen: any = {};
console.log("noLenPush=" + Array.prototype.push.call(noLen, "x") + " len=" + noLen.length + " v=" + noLen[0]);

// A string length is coerced, and the write lands past it.
const strLen: any = { length: "1", 0: "a" };
Array.prototype.push.call(strLen, "b");
console.log("strLenPush=" + strLen.length + " v=" + strLen[1] + " type=" + typeof strLen.length);

// push past 2^53-1 is a TypeError, because length cannot represent it.
const huge: any = { length: Number.MAX_SAFE_INTEGER };
try { Array.prototype.push.call(huge, "x"); } catch (e: any) { console.log("pushOverflow=" + e.constructor.name); }

// pop/shift on a frozen array are refused; the refusal is observable through
// the unchanged length rather than through a mode-dependent throw.
const frozen = Object.freeze([1, 2]);
try { frozen.pop(); } catch (e: any) { console.log("frozenPopThrew=" + (e instanceof TypeError)); }
console.log("frozenLen=" + frozen.length);

// Chaining: push returns the new length, so it is not chainable as an array.
const c: any[] = [];
console.log("returnType=" + typeof c.push(1) + " " + typeof c.pop() + " " + typeof c.shift());

// A very sparse array: shift must walk it without materialising.
const sparse: any = [];
sparse[3] = "d";
console.log("sparseShift=" + String(sparse.shift()) + " len=" + sparse.length + " keys=" + Object.keys(sparse).join(","));
