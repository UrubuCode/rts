// Cross-runtime: what `bind` does to things that are not plain functions — an
// arrow (whose `this` is already fixed), a class (which must still be
// constructible through the binding), and `bind` applied to itself.

const target: any = { tag: "target" };
const other: any = { tag: "other" };

class Reporter {
  tag: string;
  constructor(tag: string) {
    this.tag = tag;
  }
  describe(suffix: string): string {
    return this.tag + suffix;
  }
  // A field arrow captures the instance at construction time.
  arrowDescribe = (suffix: string): string => this.tag + "/arrow" + suffix;
}

// 1) Binding an arrow cannot change its `this`, but the arguments are still
//    partially applied.
const inst = new Reporter("R");
const arrowFn = inst.arrowDescribe;
const boundArrow = arrowFn.bind(other, "-pre");
console.log("arrow_bound_this_ignored=" + boundArrow());
console.log("arrow_call_this_ignored=" + arrowFn.call(other, "-call"));
console.log("arrow_apply_this_ignored=" + arrowFn.apply(other, ["-apply"]));

// 2) The bound arrow is still a new function with the usual metadata. (The
//    name is taken from an arrow in a plain binding: hosts disagree about
//    whether a CLASS FIELD arrow gets its field's name.)
const namedArrow = (a: number, b: number): number => a + b;
console.log("bound_arrow_name=" + JSON.stringify(namedArrow.bind(null).name));
console.log("bound_arrow_name_partial=" + JSON.stringify(namedArrow.bind(null, 1).name));
console.log("bound_arrow_length=" + namedArrow.bind(null, 1).length + "|arrow_length=" + namedArrow.length);
console.log("bound_arrow_is_new=" + (boundArrow !== arrowFn));

// 3) An arrow is not constructible, and neither is a bound arrow.
function tryNew(fn: any): string {
  try {
    const made = new fn(1);
    return "made:" + String(made);
  } catch (e) {
    return "threw:" + (e as any).constructor.name;
  }
}
console.log("new_arrow=" + tryNew(arrowFn));
console.log("new_bound_arrow=" + tryNew(boundArrow));

// 4) Binding a CLASS keeps it constructible; the bound `this` is ignored by
//    `new` and the bound arguments are prepended to the constructor's.
const BoundReporter: any = Reporter.bind(other);
const madeFromBound = new BoundReporter("from-bound");
console.log("bound_class_new=" + madeFromBound.describe("!"));
console.log("bound_class_instanceof=" + (madeFromBound instanceof Reporter));
console.log("bound_class_instanceof_bound=" + (madeFromBound instanceof BoundReporter));

const PartiallyBound: any = Reporter.bind(other, "fixed");
const madeFromPartial = new PartiallyBound();
console.log("partial_bound_new=" + madeFromPartial.tag);
console.log("partial_bound_length=" + PartiallyBound.length + "|class_length=" + Reporter.length);
console.log("bound_class_name=" + BoundReporter.name);

// 5) Calling a bound CLASS without `new` still fails, as the class does.
function tryCall(fn: any): string {
  try {
    return "called:" + String(fn("x"));
  } catch (e) {
    return "threw:" + (e as any).constructor.name;
  }
}
console.log("call_class=" + tryCall(Reporter));
console.log("call_bound_class=" + tryCall(BoundReporter));

// 6) The bound function's prototype chain points at the target's, so a bound
//    SUBCLASS still has its parent behind it.
class Sub extends Reporter {}
const BoundSub: any = Sub.bind(null);
console.log("bound_sub_proto_is_parent=" + (Object.getPrototypeOf(BoundSub) === Reporter));
console.log("bound_class_proto_is_function=" + (Object.getPrototypeOf(BoundReporter) === Function.prototype));
const subInstance = new BoundSub("sub");
console.log("bound_sub_instance=" + subInstance.describe("?") + "|is_reporter=" +
  (subInstance instanceof Reporter));

// 7) `bind` applied to itself: `Function.prototype.bind.bind(f)` yields a
//    function whose first argument is the `this` for the eventual bind.
// The receiver is read defensively: an unbound call sees a different value in
// each runtime's default module mode, and neither of them carries a `tag`.
function greet(suffix: string): string {
  const self: any = this;
  return (self && self.tag ? self.tag : "no-this") + suffix;
}
const bindOfGreet: any = Function.prototype.bind.bind(greet);
const boundViaBindBind = bindOfGreet(target);
console.log("bindbind_result=" + boundViaBindBind("-x"));
console.log("bindbind_name=" + boundViaBindBind.name);

// 8) `bind` called through `call` is the same operation spelled differently.
const viaCall: any = Function.prototype.bind.call(greet, target, "-fixed");
console.log("bind_via_call=" + viaCall());
console.log("bind_via_call_length=" + viaCall.length);

// 9) `bind` of `bind` used as a factory: one uncurried binder reused twice.
const binder: any = Function.prototype.bind.bind(Function.prototype.bind);
const boundToTarget = binder(greet)(target, "-A");
console.log("binder_factory=" + boundToTarget());

// 10) Rebinding an already-bound function cannot change the original `this`.
const first = greet.bind(target);
const second = first.bind(other);
console.log("rebind_keeps_this=" + second("-y"));
console.log("rebind_name=" + second.name);

// 11) Binding with no arguments at all still produces a distinct function.
const zeroBound = greet.bind(undefined);
console.log("zero_bound_this=" + zeroBound("-z"));
console.log("zero_bound_distinct=" + (zeroBound !== greet));

// 12) Two binds of the same function are different objects.
console.log("two_binds_differ=" + (greet.bind(target) !== greet.bind(target)));

// 13) A bound method used as a callback keeps its receiver.
const boundMethod = inst.describe.bind(inst);
console.log("bound_callback=" + ["-1", "-2"].map(boundMethod).join(","));

// 14) An unbound method used the same way loses it — inside a class body the
//     receiver is undefined rather than the global object.
function callLoose(): string {
  const loose = inst.describe;
  try {
    return "got:" + loose("-3");
  } catch (e) {
    return "threw:" + (e as any).constructor.name;
  }
}
console.log("unbound_callback=" + callLoose());

// 15) Binding a bound CLASS still constructs.
const DoubleBound: any = PartiallyBound.bind(null);
console.log("double_bound_new=" + new DoubleBound().tag);
console.log("double_bound_name=" + DoubleBound.name);

// 16) The bound function shares no properties with its target.
(greet as any).marker = "on-target";
const markerProbe: any = greet.bind(target);
console.log("bound_sees_marker=" + String(markerProbe.marker));
console.log("target_keeps_marker=" + (greet as any).marker);

// 17) `Reflect.construct` through a bound class uses the bound target.
const constructed: any = Reflect.construct(BoundReporter, ["via-reflect"]);
console.log("reflect_construct=" + constructed.tag + "|" + (constructed instanceof Reporter));
