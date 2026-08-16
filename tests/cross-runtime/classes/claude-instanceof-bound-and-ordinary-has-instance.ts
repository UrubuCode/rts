// Cross-runtime: instanceof falls back to OrdinaryHasInstance, which walks
// .prototype — so a bound function delegates to its target, a moved prototype
// changes the answer retroactively, and a non-callable right side is a TypeError.
class Animal {
  kind: string = "animal";
}
class Dog extends Animal {
  kind: string = "dog";
}

const d = new Dog();
console.log("dog-animal=" + (d instanceof Animal));
console.log("dog-object=" + (d instanceof Object));
console.log("dog-fn=" + (d instanceof (Function as any)));

// Function.prototype[Symbol.hasInstance] exists and is non-writable.
const hiDesc: any = Object.getOwnPropertyDescriptor(Function.prototype, Symbol.hasInstance);
console.log("hasinstance-writable=" + hiDesc.writable);
console.log("hasinstance-enumerable=" + hiDesc.enumerable);
console.log("hasinstance-configurable=" + hiDesc.configurable);
console.log("hasinstance-name=" + hiDesc.value.name);
console.log("hasinstance-length=" + hiDesc.value.length);
console.log("hasinstance-direct=" + (Function.prototype[Symbol.hasInstance] as any).call(Animal, d));

// A bound function has no .prototype, yet instanceof delegates to the target.
const BoundDog: any = Dog.bind(null);
console.log("bound-has-prototype=" + Object.prototype.hasOwnProperty.call(BoundDog, "prototype"));
console.log("bound-instanceof=" + (d instanceof BoundDog));
console.log("bound-bound-instanceof=" + (d instanceof BoundDog.bind(null)));
console.log("bound-name=" + BoundDog.name);

// A class's own "prototype" is non-writable, so it cannot be swapped at all.
const protoDesc: any = Object.getOwnPropertyDescriptor(Dog, "prototype");
console.log("class-prototype-writable=" + protoDesc.writable);
console.log("class-prototype-configurable=" + protoDesc.configurable);

// A plain function's is writable, and swapping it changes future answers only.
function Maker(this: any) {
  this.kind = "maker";
}
const madeBefore: any = new (Maker as any)();
(Maker as any).prototype = { kind: "replaced" };
console.log("after-swap-old=" + (madeBefore instanceof (Maker as any)));
const madeAfter: any = new (Maker as any)();
console.log("after-swap-new=" + (madeAfter instanceof (Maker as any)));
console.log("after-swap-new-kind=" + madeAfter.kind);

// A class with its own Symbol.hasInstance wins over the chain walk.
class Nothing {
  static [Symbol.hasInstance](): boolean {
    return false;
  }
}
class SubNothing extends Nothing {}
console.log("nothing=" + (new Nothing() as any instanceof Nothing));
console.log("sub-inherits-hasinstance=" + (new SubNothing() as any instanceof SubNothing));

// The result is coerced with ToBoolean, and the trap sees the whole value.
const seen: string[] = [];
const Tracker: any = {
  [Symbol.hasInstance](v: any): any {
    seen.push(typeof v);
    return "non-empty";
  },
};
console.log("tracker-obj=" + (({} as any) instanceof Tracker));
console.log("tracker-num=" + ((5 as any) instanceof Tracker));
console.log("tracker-null=" + ((null as any) instanceof Tracker));
console.log("tracker-seen=" + seen.join(","));

// Non-callable right side: TypeError. A plain object without the trap too.
try {
  console.log("plain-obj=" + ((d as any) instanceof ({} as any)));
} catch (e: any) {
  console.log("plain-obj=" + e.constructor.name);
}
try {
  console.log("num-rhs=" + ((d as any) instanceof (5 as any)));
} catch (e: any) {
  console.log("num-rhs=" + e.constructor.name);
}
try {
  console.log("null-rhs=" + ((d as any) instanceof (null as any)));
} catch (e: any) {
  console.log("null-rhs=" + e.constructor.name);
}

// A callable whose .prototype is a primitive: TypeError from the chain walk.
function NoProto() {}
(NoProto as any).prototype = 1;
try {
  console.log("prim-prototype=" + ((d as any) instanceof (NoProto as any)));
} catch (e: any) {
  console.log("prim-prototype=" + e.constructor.name);
}

// Function.prototype[Symbol.hasInstance] is non-writable, so an own trap has to
// be installed with defineProperty; a non-callable one is a TypeError.
const BadTrap: any = function () {};
Object.defineProperty(BadTrap, Symbol.hasInstance, { value: 7, configurable: true });
try {
  console.log("bad-trap=" + ((d as any) instanceof BadTrap));
} catch (e: any) {
  console.log("bad-trap=" + e.constructor.name);
}

// An undefined trap falls back to the ordinary .prototype walk.
const NullTrap: any = function (this: any) {};
NullTrap.prototype = Animal.prototype;
Object.defineProperty(NullTrap, Symbol.hasInstance, { value: undefined, configurable: true });
console.log("undefined-trap=" + (d instanceof NullTrap));

// Primitives are never instances of their wrapper.
console.log("string-prim=" + (("x" as any) instanceof String));
console.log("string-obj=" + (new String("x") instanceof String));
