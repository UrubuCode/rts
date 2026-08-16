// ONE thing: the three ways to build an array disagree about a single numeric
// argument, and only one of them is safe to use with unknown input.
console.log("literal=" + JSON.stringify([3]) + " len=" + [3].length);
console.log("of=" + JSON.stringify(Array.of(3)) + " len=" + Array.of(3).length);
console.log("ctor=" + JSON.stringify(new Array(3)) + " len=" + new Array(3).length);
console.log("ctorNoNew=" + JSON.stringify(Array(3)) + " len=" + Array(3).length);

// The single-argument rule only applies to a NUMBER.
console.log("ctorStr=" + JSON.stringify(Array("3")) + " len=" + Array("3").length);
console.log("ctorBool=" + JSON.stringify(Array(true)) + " len=" + Array(true).length);
console.log("ctorUndef=" + JSON.stringify(Array(undefined)) + " len=" + Array(undefined).length);
console.log("ctorNull=" + JSON.stringify(Array(null)) + " len=" + Array(null).length);
console.log("ctorObj=" + Array({ valueOf: () => 3 } as any).length);

// Two arguments are always elements, even when both are numbers.
console.log("ctorTwo=" + JSON.stringify(Array(3, 4)));
console.log("ctorZero=" + JSON.stringify(Array()) + " len=" + Array().length);

// The holes the constructor makes are real holes.
const c = new Array(3);
console.log("ctorHoles=" + [0, 1, 2].map((i) => (i in c ? "y" : "n")).join(""));
console.log("ctorForEach=" + (() => { let n = 0; c.forEach(() => n++); return n; })());
console.log("ctorMap=" + JSON.stringify(c.map(() => 1)));
console.log("ctorFill=" + JSON.stringify(c.fill(0)));

// Array.of never treats its argument as a length.
console.log("ofHoles=" + [0, 1, 2].map((i) => (i in Array.of(3) ? "y" : "n")).join("").slice(0, 1));
console.log("ofEmpty=" + Array.of().length + " ofUndef=" + Array.of(undefined).length);
console.log("ofUndefIn=" + (0 in Array.of(undefined)));

// A non-integer or negative length is a RangeError for the constructor only.
for (const v of [-1, 1.5, NaN, Infinity, 4294967296]) {
  try { console.log("ctor(" + String(v) + ")=" + Array(v as any).length); }
  catch (e: any) { console.log("ctor(" + String(v) + ")=" + e.constructor.name); }
  console.log("of(" + String(v) + ")=" + Array.of(v as any).length);
}

// Array.of and Array.from are generic over `this`.
function Box(this: any, n?: number) { this.made = true; this.length = n; }
const b: any = (Array.of as any).call(Box, "x", "y");
console.log("ofGeneric=" + b.made + " len=" + b.length + " v=" + b[0] + b[1]);
const nonCtor: any = (Array.of as any).call(undefined, 1, 2);
console.log("ofNonCtor=" + Array.isArray(nonCtor) + " len=" + nonCtor.length);

// Array.prototype is itself an array, with length 0.
console.log("protoIsArray=" + Array.isArray(Array.prototype) + " len=" + Array.prototype.length);

// The constructor property and the length of the constructor itself.
console.log("ctorLen=" + Array.length + " ofLen=" + Array.of.length + " fromLen=" + Array.from.length);
console.log("ctorName=" + Array.name + " isArrayLen=" + Array.isArray.length);
