// Pins `instanceof` with a proxy on the right: it is a get of @@hasInstance
// followed, when that is absent, by a get of "prototype" and a walk of the left
// operand's chain — so both are visible to the trap, and a non-callable proxy
// is refused before either.

const log: string[] = [];

function traced(target: any, extra?: any): any {
  return new Proxy(target, {
    get(t, k, r) {
      log.push("get:" + String(k));
      if (extra !== undefined && k in extra) return extra[k as any];
      return Reflect.get(t, k, r);
    },
    getPrototypeOf(t) { log.push("getProto"); return Reflect.getPrototypeOf(t); },
  });
}

function run(label: string, fn: () => string): void {
  log.length = 0;
  let out: string;
  try {
    out = fn();
  } catch (e: any) {
    out = "throw:" + e.constructor.name;
  }
  console.log(label + "=" + out + "|" + log.join(","));
}

class Animal { }
class Dog extends Animal { }
const dog = new Dog();

run("class_rhs", () => String(dog instanceof traced(Dog)));
run("base_rhs", () => String(dog instanceof traced(Animal)));
run("miss", () => String({} instanceof traced(Dog)));
run("primitive_lhs", () => String((5 as any) instanceof traced(Dog)));
run("null_lhs", () => String((null as any) instanceof traced(Dog)));
run("noncallable_rhs", () => String(dog instanceof (new Proxy({}, {}) as any)));
run("plain_fn_rhs", () => String(dog instanceof traced(function F() { /* noop */ })));

// the trap may substitute the prototype the walk looks for
run("substitute_prototype", () => String(dog instanceof traced(Dog, { prototype: Animal.prototype })));
run("substitute_unrelated", () => String(dog instanceof traced(Dog, { prototype: { } })));
run("bad_prototype", () => String(dog instanceof traced(Dog, { prototype: 5 })));

// @@hasInstance short-circuits the walk entirely and is called with the RHS as
// `this` and the LHS as the only argument
const hiCalls: string[] = [];
run("hasInstance_true", () => {
  const rhs: any = traced(Dog, { [Symbol.hasInstance]: function (this: any, v: any) { hiCalls.push("this_is_proxy=" + (this === rhs) + ",arg_is_dog=" + (v === dog)); return true; } });
  return String(dog instanceof rhs);
});
console.log("hasInstance_calls=" + hiCalls.join("|"));
run("hasInstance_false", () => String(dog instanceof traced(Dog, { [Symbol.hasInstance]: () => false })));
run("hasInstance_truthy", () => String(dog instanceof traced(Dog, { [Symbol.hasInstance]: () => 1 })));
run("hasInstance_zero", () => String(dog instanceof traced(Dog, { [Symbol.hasInstance]: () => 0 })));
run("hasInstance_null", () => String(dog instanceof traced(Dog, { [Symbol.hasInstance]: null })));
run("hasInstance_noncallable", () => String(dog instanceof traced(Dog, { [Symbol.hasInstance]: 5 })));
// with @@hasInstance the right operand need not be callable at all
run("hasInstance_on_object", () => String(dog instanceof (new Proxy({}, { get(_t, k) { return k === Symbol.hasInstance ? () => true : undefined; } }) as any)));

// a static @@hasInstance on the class itself is reached through the proxy
class Branded {
  static [Symbol.hasInstance](v: any): boolean { return typeof v === "string"; }
}
run("static_hasInstance", () => String(("x" as any) instanceof traced(Branded)));
run("static_hasInstance_miss", () => String((5 as any) instanceof traced(Branded)));

// a proxy on the LEFT: its own [[GetPrototypeOf]] drives the walk
const protoLog: string[] = [];
const lhsProxy: any = new Proxy(dog, { getPrototypeOf(t) { protoLog.push("proto"); return Reflect.getPrototypeOf(t); } });
console.log("lhs_proxy_dog=" + (lhsProxy instanceof Dog) + ",steps=" + protoLog.length);
protoLog.length = 0;
console.log("lhs_proxy_animal=" + (lhsProxy instanceof Animal) + ",steps=" + protoLog.length);
protoLog.length = 0;
console.log("lhs_proxy_miss=" + (lhsProxy instanceof Map) + ",steps=" + protoLog.length);

// a lying getPrototypeOf on the left makes instanceof answer differently from
// the target it wraps
const liar: any = new Proxy(dog, { getPrototypeOf() { return Map.prototype; } });
console.log("lying_lhs_map=" + (liar instanceof Map) + ",dog=" + (liar instanceof Dog));

// both operands proxies
console.log("both=" + (new Proxy(dog, {}) instanceof (new Proxy(Dog, {}) as any)));
// a bound proxy of a class keeps the target's prototype for the walk
const boundCtor: any = (Dog as any).bind(null);
console.log("bound_rhs=" + (dog instanceof (new Proxy(boundCtor, {}) as any)));
