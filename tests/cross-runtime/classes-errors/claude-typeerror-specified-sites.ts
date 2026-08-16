// Cross-runtime: the TypeError sites the specification names. Every refused
// WRITE is probed inside a class body, which is strict whatever module goal the
// host picked for this file, and again through Reflect.set, which is strictness-
// independent. Only the error CONSTRUCTOR is printed, never the message.
class Probe {
  static run(fn: () => any): string {
    try {
      const v = fn();
      return "ok:" + String(v);
    } catch (e: any) {
      return e.constructor.name;
    }
  }
  static write(target: any, key: any, value: any): string {
    try {
      target[key] = value;
      return "no-throw";
    } catch (e: any) {
      return e.constructor.name;
    }
  }
  static del(target: any, key: any): string {
    try {
      delete target[key];
      return "no-throw";
    } catch (e: any) {
      return e.constructor.name;
    }
  }
}

class Klass {
  v: number = 1;
}
console.log("class-no-new=" + Probe.run(() => (Klass as any)()));
console.log("class-reflect-apply=" + Probe.run(() => Reflect.apply(Klass as any, undefined, [])));

const arrow = () => 1;
console.log("new-arrow=" + Probe.run(() => new (arrow as any)()));
const holder = {
  m(): number {
    return 1;
  },
  get g(): number {
    return 1;
  },
};
console.log("new-method=" + Probe.run(() => new (holder.m as any)()));
const gDesc: any = Object.getOwnPropertyDescriptor(holder, "g");
console.log("new-getter=" + Probe.run(() => new (gDesc.get as any)()));
console.log("new-plain-object=" + Probe.run(() => new ({} as any)()));

console.log("prop-of-undefined=" + Probe.run(() => (undefined as any).x));
console.log("prop-of-null=" + Probe.run(() => (null as any).x));
console.log("call-of-undefined=" + Probe.run(() => ({} as any).nope()));
console.log("destructure-null=" + Probe.run(() => { const { a } = null as any; return a; }));
console.log("spread-null=" + Probe.run(() => [...(null as any)]));

const sym = Symbol("s");
console.log("symbol-plus-string=" + Probe.run(() => (sym as any) + "x"));
console.log("symbol-template=" + Probe.run(() => `${sym as any}`));
console.log("symbol-number=" + Probe.run(() => Number(sym as any)));
console.log("symbol-string-ok=" + Probe.run(() => String(sym)));

console.log("bigint-plus-number=" + Probe.run(() => (1n as any) + 1));
console.log("bigint-number-mul=" + Probe.run(() => (2n as any) * 3));
console.log("bigint-compare-ok=" + Probe.run(() => 1n < 2));
console.log("bigint-number-ctor-ok=" + Probe.run(() => Number(1n)));
console.log("number-to-bigint=" + Probe.run(() => BigInt(1.5)));
console.log("bigint-unary-plus=" + Probe.run(() => +(1n as any)));

const frozen = Object.freeze({ a: 1 });
console.log("frozen-write=" + Probe.write(frozen, "a", 2));
console.log("frozen-add=" + Probe.write(frozen, "b", 1));
console.log("frozen-delete=" + Probe.del(frozen, "a"));
console.log("frozen-value=" + frozen.a);

const sealed = Object.seal({ a: 1 });
console.log("sealed-write=" + Probe.write(sealed, "a", 2));
console.log("sealed-add=" + Probe.write(sealed, "b", 1));
console.log("sealed-delete=" + Probe.del(sealed, "a"));
console.log("sealed-value=" + sealed.a);

const nonExt: any = Object.preventExtensions({ a: 1 });
console.log("nonext-write=" + Probe.write(nonExt, "a", 2));
console.log("nonext-add=" + Probe.write(nonExt, "b", 1));

console.log("defineproperty-primitive=" + Probe.run(() => Object.defineProperty(5 as any, "x", { value: 1 })));
console.log("defineproperty-null=" + Probe.run(() => Object.defineProperty(null as any, "x", { value: 1 })));
console.log("defineproperty-bad-desc=" + Probe.run(() => Object.defineProperty({}, "x", 5 as any)));
console.log("defineproperty-both-kinds=" + Probe.run(() => Object.defineProperty({}, "x", { value: 1, get() { return 1; } } as any)));
console.log("setprototypeof-primitive-ok=" + Probe.run(() => Object.setPrototypeOf(5 as any, null)));
console.log("getprototypeof-null=" + Probe.run(() => Object.getPrototypeOf(null as any)));

// Redefining a non-configurable property.
const locked: any = {};
Object.defineProperty(locked, "k", { value: 1, configurable: false, writable: false });
console.log("redefine-locked=" + Probe.run(() => Object.defineProperty(locked, "k", { value: 2 })));
console.log("redefine-same-ok=" + Probe.run(() => Object.defineProperty(locked, "k", { value: 1 })));

// A cyclic prototype, and setting a prototype on a non-extensible object.
const a: any = {};
const b: any = Object.create(a);
console.log("cyclic-proto=" + Probe.run(() => Object.setPrototypeOf(a, b)));
const sealedProto: any = Object.preventExtensions({});
console.log("proto-on-nonext=" + Probe.run(() => Object.setPrototypeOf(sealedProto, { x: 1 })));

// Reflect variants answer false instead of throwing. This is the mode-independent
// way to pin the same refusals: the boolean plus a re-read of the property says
// what happened without depending on the caller being strict.
function attempt(target: any, key: string, value: any): string {
  const accepted = Reflect.set(target, key, value);
  return accepted + ":" + String(target[key]);
}
console.log("reflect-set-frozen=" + attempt(frozen, "a", 2));
console.log("reflect-add-frozen=" + attempt(frozen, "b", 1));
console.log("reflect-set-sealed=" + attempt(sealed, "a", 3));
console.log("reflect-add-sealed=" + attempt(sealed, "c", 1));
console.log("reflect-add-nonext=" + attempt(nonExt, "d", 1));
console.log("reflect-define-locked=" + Reflect.defineProperty(locked, "k", { value: 2 }));
console.log("reflect-locked-value=" + locked.k);
console.log("reflect-delete-frozen=" + Reflect.deleteProperty(frozen, "a"));
console.log("reflect-delete-sealed=" + Reflect.deleteProperty(sealed, "a"));
console.log("reflect-setproto-cyclic=" + Reflect.setPrototypeOf(a, b));
console.log("reflect-setproto-nonext=" + Reflect.setPrototypeOf(sealedProto, { x: 1 }));

// A method borrowed onto the wrong receiver.
console.log("map-wrong-receiver=" + Probe.run(() => (Map.prototype.get as any).call({}, 1)));
console.log("set-wrong-receiver=" + Probe.run(() => (Set.prototype.has as any).call([], 1)));
console.log("date-wrong-receiver=" + Probe.run(() => (Date.prototype.getTime as any).call({})));
console.log("weakmap-primitive-key=" + Probe.run(() => new WeakMap().set(1 as any, 1)));
console.log("bind-non-callable=" + Probe.run(() => (Function.prototype.bind as any).call({}, null)));
