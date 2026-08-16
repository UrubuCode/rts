// Cross-runtime: the TypeErrors the `instanceof` and `in` operators raise, and
// the exact order they check their operands in. `instanceof` refuses a
// non-object right side before it refuses a non-callable one; `in` refuses any
// primitive right side outright.
function probe(fn: () => any): string {
  try {
    const v = fn();
    return "ok:" + String(v);
  } catch (e: any) {
    return e.constructor.name;
  }
}

class Thing {}
const t = new Thing();

// instanceof with a right side that is not an object at all.
console.log("inst-number=" + probe(() => (t as any) instanceof (1 as any)));
console.log("inst-string=" + probe(() => (t as any) instanceof ("Thing" as any)));
console.log("inst-null=" + probe(() => (t as any) instanceof (null as any)));
console.log("inst-undefined=" + probe(() => (t as any) instanceof (undefined as any)));
console.log("inst-symbol=" + probe(() => (t as any) instanceof (Symbol("s") as any)));

// An object that is not callable and has no Symbol.hasInstance.
console.log("inst-plain-object=" + probe(() => (t as any) instanceof ({} as any)));
console.log("inst-array=" + probe(() => (t as any) instanceof ([] as any)));

// A primitive LEFT side is not an error — it is simply false.
console.log("inst-left-number=" + probe(() => (1 as any) instanceof Thing));
console.log("inst-left-null=" + probe(() => (null as any) instanceof Thing));
console.log("inst-left-undefined=" + probe(() => (undefined as any) instanceof Thing));
console.log("inst-left-string=" + probe(() => ("x" as any) instanceof String));
console.log("inst-boxed-string=" + probe(() => new String("x") instanceof String));

// Symbol.hasInstance takes over completely — including making a NON-callable
// object a legal right operand.
const acceptsAll: any = {
  [Symbol.hasInstance](v: any): boolean {
    return typeof v === "number";
  },
};
console.log("hasinstance-object=" + probe(() => (1 as any) instanceof acceptsAll));
console.log("hasinstance-object-false=" + probe(() => ("1" as any) instanceof acceptsAll));

// A Symbol.hasInstance that is present but not callable is a TypeError.
const badHasInstance: any = { [Symbol.hasInstance]: 5 };
console.log("hasinstance-not-callable=" + probe(() => (1 as any) instanceof badHasInstance));

// An explicitly undefined Symbol.hasInstance falls back to the ordinary
// algorithm, so a plain object right side is refused again.
const undefHasInstance: any = { [Symbol.hasInstance]: undefined };
console.log("hasinstance-undefined=" + probe(() => (1 as any) instanceof undefHasInstance));

// A hasInstance that throws propagates its own error unchanged.
const throwing: any = {
  [Symbol.hasInstance](): boolean {
    throw new RangeError("mine");
  },
};
console.log("hasinstance-throws=" + probe(() => (1 as any) instanceof throwing));

// The ordinary path reads `prototype` off the constructor and refuses a
// non-object one.
function NoProto(): void {
  // nothing
}
(NoProto as any).prototype = 5;
console.log("bad-prototype=" + probe(() => (t as any) instanceof (NoProto as any)));
(NoProto as any).prototype = null;
console.log("null-prototype=" + probe(() => (t as any) instanceof (NoProto as any)));

// A class static Symbol.hasInstance replaces the brand check on that class.
class Branded {
  static [Symbol.hasInstance](v: any): boolean {
    return typeof v === "object" && v !== null && "brand" in v;
  }
}
console.log("branded-yes=" + probe(() => ({ brand: 1 } as any) instanceof Branded));
console.log("branded-no=" + probe(() => ({} as any) instanceof Branded));
console.log("branded-real-instance=" + probe(() => new Branded() instanceof Branded));

// `in` refuses EVERY primitive right operand.
console.log("in-number=" + probe(() => "x" in (1 as any)));
console.log("in-string=" + probe(() => "0" in ("abc" as any)));
console.log("in-null=" + probe(() => "x" in (null as any)));
console.log("in-undefined=" + probe(() => "x" in (undefined as any)));
console.log("in-boolean=" + probe(() => "x" in (true as any)));
console.log("in-symbol=" + probe(() => "x" in (Symbol("s") as any)));
console.log("in-boxed-string=" + probe(() => "0" in (new String("abc") as any)));

// `in` on real objects walks the prototype chain and accepts symbol keys.
console.log("in-own=" + probe(() => "brand" in ({ brand: 1 } as any)));
console.log("in-inherited=" + probe(() => "toString" in ({} as any)));
console.log("in-class-method=" + probe(() => "constructor" in t));
console.log("in-symbol-key=" + probe(() => Symbol.iterator in ([] as any)));
console.log("in-array-index=" + probe(() => 0 in ([1] as any)));
console.log("in-array-hole=" + probe(() => 0 in ([, 1] as any)));

// A private-name `in` check answers false off-brand instead of throwing, but
// it still demands an OBJECT on the right.
class Priv {
  #secret: number = 1;
  static has(v: any): boolean {
    return #secret in v;
  }
  read(): number {
    return this.#secret;
  }
}
console.log("private-in-self=" + probe(() => Priv.has(new Priv())));
console.log("private-in-plain=" + probe(() => Priv.has({})));
console.log("private-in-primitive=" + probe(() => Priv.has(1)));
console.log("private-in-null=" + probe(() => Priv.has(null)));
console.log("private-read-offbrand=" + probe(() => (Priv.prototype.read as any).call({})));
