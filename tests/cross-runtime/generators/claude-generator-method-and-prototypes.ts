// Cross-runtime: a generator METHOD on a class, `*[Symbol.iterator]()` making
// the instance iterable, and the four-level prototype chain every generator
// object sits on. Focus: shapes and identities, not just the values yielded.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

class Bag {
  items: number[];
  constructor(items: number[]) { this.items = items; }

  // a generator method: iterating an instance walks its items
  *[Symbol.iterator]() {
    for (let i = 0; i < this.items.length; i++) yield this.items[i];
  }

  // an ordinary generator method, reached by name
  *doubled() {
    for (const v of this) yield v * 2;
  }

  // a static generator method
  static *range(k: number) {
    for (let i = 0; i < k; i++) yield i;
  }
}

const bag = new Bag([1, 2, 3]);

// 1) the instance is iterable through the generator method
log("forOf=" + Array.from(bag).join(","));
log("spread=" + [...bag].join(","));
log("doubled=" + [...bag.doubled()].join(","));
log("static=" + [...Bag.range(4)].join(","));

// 2) `this` inside the generator method is the instance, resolved at CALL time
const detached = Bag.prototype[Symbol.iterator];
const borrowed = detached.call({ items: [7, 8] });
log("borrowed=" + Array.from(borrowed as any).join(","));

// 3) each call makes a fresh, independent generator object
const g1 = bag[Symbol.iterator]();
const g2 = bag[Symbol.iterator]();
log("freshEachCall=" + (g1 !== g2));
log("g1first=" + g1.next().value + " g2first=" + g2.next().value);
log("independent=" + g1.next().value + "," + g2.next().value);

// 4) a generator object is its OWN iterable, and Symbol.iterator returns this
const g3 = Bag.range(2);
log("selfIterable=" + ((g3 as any)[Symbol.iterator]() === g3));
log("onGeneratorPrototype=" + Object.prototype.hasOwnProperty.call(g3, Symbol.iterator));

// 5) the prototype chain: generator object -> method's .prototype ->
//    %GeneratorPrototype% -> %IteratorPrototype%
const own = Object.getPrototypeOf(g3);
const genProto = Object.getPrototypeOf(own);
const iterProto = Object.getPrototypeOf(genProto);
log("ownIsMethodPrototype=" + (own === (Bag.range as any).prototype));
log("genProtoHasNext=" + Object.prototype.hasOwnProperty.call(genProto, "next"));
log("genProtoHasThrow=" + Object.prototype.hasOwnProperty.call(genProto, "throw"));
log("genProtoHasReturn=" + Object.prototype.hasOwnProperty.call(genProto, "return"));
log("iterProtoHasSymbolIterator=" + Object.prototype.hasOwnProperty.call(iterProto, Symbol.iterator));
log("iterProtoParent=" + (Object.getPrototypeOf(iterProto) === Object.prototype));

// 6) Symbol.toStringTag on the generator prototypes
log("genTag=" + genProto[Symbol.toStringTag]);
log("toStringOfGenerator=" + Object.prototype.toString.call(g3));

// 7) a generator function's own shape
function* plain() { yield 1; }
log("typeofGenFn=" + typeof plain);
log("genFnTag=" + Object.getPrototypeOf(plain)[Symbol.toStringTag]);
log("toStringOfGenFn=" + Object.prototype.toString.call(plain));
log("genFnHasPrototype=" + (typeof plain.prototype));
log("prototypeIsNotConstructable=" + (function () {
  try { new (plain as any)(); return "no"; } catch (e: any) { return e.constructor.name; }
})());

// 8) the shared %GeneratorFunction.prototype% across declarations
function* other() { yield 2; }
log("sharedGenFnProto=" + (Object.getPrototypeOf(plain) === Object.getPrototypeOf(other)));
log("sharedGenProto=" + (Object.getPrototypeOf(plain.prototype) === Object.getPrototypeOf(other.prototype)));
log("methodSharesIt=" + (Object.getPrototypeOf((Bag.range as any).prototype) === Object.getPrototypeOf(plain.prototype)));

// 9) a generator method is NOT enumerable on the prototype and has no
//    constructor behaviour
log("methodEnumerable=" + Object.getOwnPropertyDescriptor(Bag.prototype, "doubled").enumerable);
log("methodName=" + (Bag.prototype as any).doubled.name);
log("iteratorMethodName=" + JSON.stringify(Bag.prototype[Symbol.iterator].name));

// 10) an object literal generator method behaves the same way
const lit = {
  *gen() { yield "l1"; yield "l2"; },
  *[Symbol.iterator]() { yield* (this as any).gen(); }
};
log("literalSpread=" + [...lit].join(","));
log("literalMethodName=" + (lit as any).gen.name);

console.log("end");
