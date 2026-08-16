// Cross-runtime: building errors through Reflect.construct. The prototype
// comes from newTarget, the internals (message, cause, errors) come from the
// constructor that ran, and the two can disagree — an object whose `name` says
// one thing and whose brand says another.
class Tagged extends Error {
  tag: string = "tagged";
}

// newTarget decides the prototype; Error decides everything else.
const asTagged: any = Reflect.construct(Error, ["m1"], Tagged);
console.log("proto-is-tagged=" + (Object.getPrototypeOf(asTagged) === Tagged.prototype));
console.log("instanceof-tagged=" + (asTagged instanceof Tagged));
console.log("instanceof-error=" + (asTagged instanceof Error));
console.log("ctor-name=" + asTagged.constructor.name);
console.log("name=" + asTagged.name);
console.log("message=" + asTagged.message);
console.log("tag-field=" + String(asTagged.tag));
console.log("keys=" + Object.keys(asTagged).join(","));
console.log("tostring=" + asTagged.toString());
console.log("tag-brand=" + Object.prototype.toString.call(asTagged));

// The Tagged constructor never ran, so its field is absent — Reflect.construct
// on Tagged itself installs it.
const realTagged: any = Reflect.construct(Tagged, ["m2"]);
console.log("real-tag=" + realTagged.tag);
console.log("real-keys=" + Object.keys(realTagged).join(","));
console.log("real-message=" + realTagged.message);
console.log("real-name=" + realTagged.name);

// A TypeError body under an Error prototype: `name` follows the PROTOTYPE, and
// instanceof follows it too, so a TypeError-built object can answer false to
// `instanceof TypeError`.
const crossed: any = Reflect.construct(TypeError, ["m3"], RangeError);
console.log("crossed-name=" + crossed.name);
console.log("crossed-tostring=" + crossed.toString());
console.log("crossed-instanceof-range=" + (crossed instanceof RangeError));
console.log("crossed-instanceof-type=" + (crossed instanceof TypeError));
console.log("crossed-instanceof-error=" + (crossed instanceof Error));
console.log("crossed-proto=" + (Object.getPrototypeOf(crossed) === RangeError.prototype));

// cause travels with the constructor's options argument, not with newTarget.
const withCause: any = Reflect.construct(Error, ["m4", { cause: "root" }], Tagged);
console.log("cause-own=" + Object.prototype.hasOwnProperty.call(withCause, "cause"));
console.log("cause-value=" + withCause.cause);
const cd: any = Object.getOwnPropertyDescriptor(withCause, "cause");
console.log("cause-desc=w" + cd.writable + ",e" + cd.enumerable + ",c" + cd.configurable);
const noCause: any = Reflect.construct(Error, ["m5"], Tagged);
console.log("no-cause-own=" + Object.prototype.hasOwnProperty.call(noCause, "cause"));

// AggregateError through Reflect.construct keeps the errors list on the
// instance while the prototype comes from newTarget.
class Group extends AggregateError {}
const agg: any = Reflect.construct(AggregateError, [[new TypeError("a"), new RangeError("b")], "m6"], Group);
console.log("agg-proto=" + (Object.getPrototypeOf(agg) === Group.prototype));
console.log("agg-errors-len=" + agg.errors.length);
console.log("agg-errors-kinds=" + agg.errors.map((e: any) => e.constructor.name).join(","));
console.log("agg-name=" + agg.name);
console.log("agg-message=" + agg.message);
console.log("agg-instanceof=" + (agg instanceof Group) + "," + (agg instanceof AggregateError) + "," + (agg instanceof Error));
console.log("agg-errors-own=" + Object.prototype.hasOwnProperty.call(agg, "errors"));

// A newTarget whose prototype is not an object falls back to the intrinsic
// prototype of the running constructor.
function Odd(): void {
  // nothing
}
(Odd as any).prototype = 5;
const fallback: any = Reflect.construct(RangeError, ["m7"], Odd);
console.log("fallback-proto=" + (Object.getPrototypeOf(fallback) === RangeError.prototype));
console.log("fallback-name=" + fallback.name);
console.log("fallback-instanceof=" + (fallback instanceof RangeError));

// A non-constructor as target or newTarget is a TypeError.
function probe(fn: () => any): string {
  try {
    const v = fn();
    return "ok:" + String(v);
  } catch (e: any) {
    return e.constructor.name;
  }
}
const arrow = () => 1;
console.log("target-arrow=" + probe(() => Reflect.construct(arrow as any, [])));
console.log("newtarget-arrow=" + probe(() => Reflect.construct(Error, [], arrow as any)));
console.log("target-primitive=" + probe(() => Reflect.construct(5 as any, [])));
console.log("args-not-array=" + probe(() => Reflect.construct(Error, 5 as any)));
console.log("args-arraylike=" + probe(() => Reflect.construct(Error, { length: 1, 0: "from-arraylike" } as any).message));

// Subclassing while calling super with Reflect-provided arguments works the
// ordinary way, and the subclass prototype repair is unnecessary.
class Wrapped extends Error {
  detail: string = "";
  constructor(message: string, detail: string) {
    super(message, { cause: detail });
    this.name = "Wrapped";
    this.detail = detail;
  }
}
const w: any = Reflect.construct(Wrapped, ["m8", "d8"]);
console.log("wrapped-name=" + w.name);
console.log("wrapped-detail=" + w.detail);
console.log("wrapped-cause=" + w.cause);
console.log("wrapped-keys=" + Object.keys(w).sort().join(","));
console.log("wrapped-instanceof=" + (w instanceof Wrapped) + "," + (w instanceof Error));
console.log("wrapped-proto-ctor=" + (Wrapped.prototype.constructor === Wrapped));
