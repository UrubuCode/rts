// Cross-runtime: WHO may call an iterator helper. The methods are generic over
// any object with `next`, the helper objects themselves are branded and refuse a
// foreign `this`, and `Iterator` cannot be called or constructed directly.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

function attempt(fn: () => any): string {
  try { const v = fn(); return "ok:" + (v === undefined ? "undefined" : String(v)); }
  catch (e: any) { return e.constructor.name; }
}

const IterProto: any = (Iterator as any).prototype;

// 1) Iterator is a constructor but an ABSTRACT one: neither call nor `new`
log("typeofIterator=" + typeof Iterator);
log("directCall=" + attempt(function () { return (Iterator as any)(); }));
log("directNew=" + attempt(function () { return new (Iterator as any)(); }));

// 2) a subclass CAN be constructed, and inherits every helper
class Counter extends (Iterator as any) {
  i: number;
  limit: number;
  constructor(limit: number) { super(); this.i = 0; this.limit = limit; }
  next() { return this.i < this.limit ? { done: false, value: this.i++ } : { done: true, value: undefined }; }
}
const c = new Counter(5) as any;
log("subclassWorks=" + c.map(function (v: number) { return v * 2; }).take(3).toArray().join(","));
log("subclassIsIterator=" + (c instanceof (Iterator as any)));
log("subclassSpread=" + [...(new Counter(3) as any)].join(","));

// 3) the helper methods are GENERIC: a plain object with `next` is enough
const plain = { i: 0, next: function () { return this.i < 3 ? { done: false, value: "p" + this.i++ } : { done: true, value: undefined }; } };
log("genericMap=" + IterProto.map.call(plain, function (v: string) { return v + "!"; }).toArray().join(","));

// 4) but the receiver must be an OBJECT
log("nullThis=" + attempt(function () { return IterProto.map.call(null, function (v: any) { return v; }); }));
log("numberThis=" + attempt(function () { return IterProto.map.call(5, function (v: any) { return v; }); }));
log("stringThis=" + attempt(function () { return IterProto.map.call("abc", function (v: any) { return v; }); }));

// 5) an object with NO next is accepted at build time and fails on first pull
const noNext: any = IterProto.map.call({}, function (v: any) { return v; });
log("noNextBuilt=" + (typeof noNext));
log("noNextPull=" + attempt(function () { return noNext.next(); }));

// 6) the helper objects are BRANDED: their next/return refuse a foreign `this`
function* g() { yield 1; }
const helper: any = g().map(function (v: number) { return v; });
const helperProto = Object.getPrototypeOf(helper);
log("brandedNext=" + attempt(function () { return helperProto.next.call({}); }));
log("brandedReturn=" + attempt(function () { return helperProto.return.call({}); }));
log("brandedOnGenerator=" + attempt(function () { return helperProto.next.call(g()); }));
log("brandedOnItself=" + JSON.stringify(helperProto.next.call(helper)));

// 7) a generator's own next refuses a helper as `this`
const genProto: any = Object.getPrototypeOf(Object.getPrototypeOf(g()));
log("generatorNextOnHelper=" + attempt(function () { return genProto.next.call(g().map(function (v: any) { return v; })); }));

// 8) Iterator.prototype[Symbol.iterator] answers the receiver unchanged
log("symbolIteratorIdentity=" + (IterProto[Symbol.iterator].call(plain) === plain));
log("symbolIteratorOnPrimitive=" + attempt(function () { return IterProto[Symbol.iterator].call(1); }));

// 9) Iterator.from on a plain next-object hands back a WRAPPER, and on
//    something already an Iterator instance hands the thing itself back
const wrapped: any = (Iterator as any).from(plain);
log("fromWrapsPlain=" + (wrapped !== plain) + " hasMap=" + (typeof wrapped.map));
log("fromKeepsIterator=" + ((Iterator as any).from(c) === c));
log("fromOnIterable=" + (Iterator as any).from([1, 2, 3]).toArray().join(","));
log("fromOnPrimitive=" + attempt(function () { return (Iterator as any).from(5); }));
log("fromOnString=" + (Iterator as any).from("ab").toArray().join(","));

// 10) `constructor` and `Symbol.toStringTag` on Iterator.prototype are
//     ACCESSORS, not data properties
const ctorDesc: any = Object.getOwnPropertyDescriptor(IterProto, "constructor");
const tagDesc: any = Object.getOwnPropertyDescriptor(IterProto, Symbol.toStringTag);
log("constructorIsAccessor=" + (typeof ctorDesc.get) + "," + (typeof ctorDesc.set));
log("tagIsAccessor=" + (typeof tagDesc.get) + "," + (typeof tagDesc.set));
log("tagValue=" + IterProto[Symbol.toStringTag] + " ctorValue=" + (IterProto.constructor === Iterator));

// 11) those setters do NOT throw when the receiver is a plain object -- they
//     define an own property on it instead
const receiver: any = Object.create(IterProto);
receiver[Symbol.toStringTag] = "Mine";
log("tagOnReceiver=" + receiver[Symbol.toStringTag] + " ownNow=" + Object.prototype.hasOwnProperty.call(receiver, Symbol.toStringTag));
log("protoTagUntouched=" + IterProto[Symbol.toStringTag]);

// 12) an exhausted helper answers done forever, and a helper built on it
//     yields nothing
const spent: any = g().map(function (v: number) { return v; });
spent.toArray();
log("spentNext=" + JSON.stringify(spent.next()));
log("helperOnSpent=" + JSON.stringify(spent.map(function (v: any) { return v; }).toArray()));

// 13) a helper built on an already-consumed GENERATOR is empty too
const spentGen = g();
spentGen.next(); spentGen.next();
log("helperOnSpentGenerator=" + JSON.stringify(spentGen.map(function (v: number) { return v; }).toArray()));

console.log("end");
