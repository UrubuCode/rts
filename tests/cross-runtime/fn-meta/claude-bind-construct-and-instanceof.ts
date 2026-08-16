// Cross-runtime: `new` through a bound function IGNORES the bound `this` but
// keeps the bound arguments, and `instanceof` resolves against the TARGET's
// prototype, not the bound wrapper.

function Pair(this: any, a: number, b: number) {
  this.a = a;
  this.b = b;
  this.tagged = (this as any).tagged === undefined ? "fresh" : "reused";
}
Pair.prototype.sum = function (this: any): number { return this.a + this.b; };
Pair.prototype.kind = "Pair";

const bad = { a: -1, b: -1, tagged: "bound-this" };
const BoundPair: any = (Pair as any).bind(bad);

// Called as a function, the bound `this` is used.
BoundPair(7, 8);
console.log("call_used_bound_this=" + bad.a + "," + bad.b + "," + bad.tagged);

// Constructed, the bound `this` is discarded for a fresh object.
const made = new BoundPair(1, 2);
console.log("new_ignored_bound_this=" + made.a + "," + made.b);
console.log("new_fresh=" + made.tagged);
console.log("bad_untouched_by_new=" + bad.a + "," + bad.b);

// Bound arguments still prepend under `new`.
const BoundOne: any = (Pair as any).bind(bad, 10);
const partial = new BoundOne(20);
console.log("bound_arg_kept=" + partial.a + "," + partial.b);
console.log("bound_arg_sum=" + partial.sum());

// Both arguments bound: `new` needs none.
const BoundBoth: any = (Pair as any).bind(bad, 3, 4);
console.log("both_bound=" + new BoundBoth().sum());
console.log("extra_args_ignored=" + new BoundBoth(99, 99).sum());

// instanceof resolves through the target's prototype.
console.log("instance_of_target=" + (made instanceof (Pair as any)));
console.log("instance_of_bound=" + (made instanceof BoundPair));
console.log("proto_is_target_proto=" + (Object.getPrototypeOf(made) === Pair.prototype));
console.log("inherited_kind=" + made.kind);

// An ordinary object made by the target is also `instanceof` the bound wrapper.
const direct = new (Pair as any)(5, 6);
console.log("direct_instanceof_bound=" + (direct instanceof BoundPair));

// Bound functions have no `prototype`, so `new` cannot use one.
console.log("bound_has_prototype=" + Object.prototype.hasOwnProperty.call(BoundPair, "prototype"));

// Binding a bound constructor still reaches the original target.
const Twice: any = BoundOne.bind(bad, 30);
const twiceMade = new Twice();
console.log("double_bound=" + twiceMade.a + "," + twiceMade.b);
console.log("double_bound_instanceof=" + (twiceMade instanceof (Pair as any)));

// Classes bind the same way.
class Box {
  v: number;
  constructor(v: number) { this.v = v; }
  read(): string { return "box:" + this.v; }
}
const BoundBox: any = (Box as any).bind(null, 42);
const boxed = new BoundBox();
console.log("class_bound=" + boxed.read());
console.log("class_bound_instanceof=" + (boxed instanceof Box));
console.log("class_bound_proto=" + (Object.getPrototypeOf(boxed) === Box.prototype));

// A bound class still refuses a plain call, as the class itself does.
try {
  BoundBox();
  console.log("bound_class_call=ok");
} catch (e) {
  console.log("bound_class_call_threw=" + (e as any).constructor.name);
}

// Subclassing keeps working through the bound base's instances.
class Deeper extends Box {
  extra = "d";
  describe(): string { return this.read() + "/" + this.extra; }
}
const deep = new Deeper(9);
console.log("subclass=" + deep.describe());
console.log("subclass_instanceof_bound_base=" + (deep instanceof BoundBox));

// `Symbol.hasInstance` on the TARGET is what a bound wrapper consults.
class Branded {
  static [Symbol.hasInstance](x: any): boolean { return x != null && x.brand === "yes"; }
}
const BoundBranded: any = (Branded as any).bind(null);
console.log("has_instance_target=" + ({ brand: "yes" } instanceof Branded));
console.log("has_instance_bound=" + (({ brand: "yes" } as any) instanceof BoundBranded));
console.log("has_instance_bound_no=" + (({ brand: "no" } as any) instanceof BoundBranded));

// `new.target` inside the target names the bound wrapper's target.
function ReportsTarget(this: any): void {
  this.sawTarget = new.target === undefined ? "none" : (new.target === ReportsTarget ? "self" : "other");
}
const BoundReport: any = (ReportsTarget as any).bind(null);
console.log("new_target_via_bound=" + new BoundReport().sawTarget);
const asFn: any = {};
(ReportsTarget as any).call(asFn);
console.log("new_target_plain_call=" + asFn.sawTarget);
