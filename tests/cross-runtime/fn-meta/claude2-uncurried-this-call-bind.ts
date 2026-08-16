// Cross-runtime: the "uncurried this" idiom — `Function.prototype.call.bind(f)`
// turns a method into a plain function whose FIRST argument is the receiver.
// What the derived function reports as its own name and length is the
// assertion, alongside the borrowing itself working.

const uncurry = Function.prototype.bind.bind(Function.prototype.call);

// 1) A borrowed predicate: the receiver becomes an ordinary first argument.
const hasOwn: any = Function.prototype.call.bind(Object.prototype.hasOwnProperty);
const sample: any = { present: 1 };
console.log("hasOwn_true=" + hasOwn(sample, "present"));
console.log("hasOwn_false=" + hasOwn(sample, "missing"));
console.log("hasOwn_on_bare=" + hasOwn(Object.create(null), "anything"));

// 2) The same idiom spelled with the uncurry helper.
const toStringOf: any = uncurry(Object.prototype.toString);
console.log("tag_array=" + toStringOf([]));
console.log("tag_null=" + toStringOf(null));
console.log("tag_number=" + toStringOf(3));
console.log("tag_date_like=" + toStringOf(new Map()));

// 3) A borrowed array method over an array-like.
const slice: any = Function.prototype.call.bind(Array.prototype.slice);
const join: any = Function.prototype.call.bind(Array.prototype.join);
const arrayLike: any = { length: 3, 0: "a", 1: "b", 2: "c" };
console.log("borrowed_slice=" + JSON.stringify(slice(arrayLike, 1)));
console.log("borrowed_join=" + join(arrayLike, "-"));
console.log("borrowed_on_string=" + join(Object("xyz"), "+"));

// 4) The derived function's metadata comes from `call`, not from the target.
console.log("derived_name=" + JSON.stringify(hasOwn.name));
console.log("derived_length=" + hasOwn.length);
console.log("call_length=" + Function.prototype.call.length);
console.log("target_length=" + Object.prototype.hasOwnProperty.length);

// 5) `apply` bound the same way takes an ARRAY of arguments after the receiver.
const applyOf: any = Function.prototype.apply.bind(Math.max);
console.log("apply_bound_max=" + applyOf(null, [3, 9, 4]));
console.log("apply_bound_max_empty=" + applyOf(null, []));

// 6) Binding the receiver as well gives a plain variadic function.
const maxOf: any = Function.prototype.apply.bind(Math.max, null);
console.log("max_of_array=" + maxOf([1, 8, 2]));
console.log("min_of_array=" + Function.prototype.apply.bind(Math.min, null)([1, 8, 2]));

// 7) `call.call` reaches one level further: the first argument is the function.
console.log("call_call=" + Function.prototype.call.call(Object.prototype.hasOwnProperty, sample, "present"));
console.log("call_apply=" + Function.prototype.apply.call(Math.max, null, [5, 6]));

// 8) A borrowed method still respects a receiver it was not written for, as
//     long as the shape fits.
const numberToFixed: any = Function.prototype.call.bind(Number.prototype.toFixed);
console.log("borrowed_toFixed=" + numberToFixed(1.2345, 2));
function borrowedOnWrongType(): string {
  try {
    return "ok:" + numberToFixed("1.2345", 2);
  } catch (e) {
    return "threw:" + (e as any).constructor.name;
  }
}
console.log("wrong_receiver=" + borrowedOnWrongType());

// 9) The bound helper is a distinct object each time it is made.
console.log("distinct_helpers=" + (Function.prototype.call.bind(Object.prototype.hasOwnProperty) !==
  Function.prototype.call.bind(Object.prototype.hasOwnProperty)));

// 10) A helper built from a class method works on instances only.
class Counter {
  n = 0;
  bump(by: number): number {
    this.n += by;
    return this.n;
  }
}
const bump: any = Function.prototype.call.bind(Counter.prototype.bump);
const counter = new Counter();
console.log("borrowed_class_method=" + bump(counter, 2) + "," + bump(counter, 3));
console.log("borrowed_on_plain=" + bump({ n: 10 } as any, 5));

// 11) `Reflect.apply` is the same operation without the borrowing dance.
console.log("reflect_apply=" + Reflect.apply(Object.prototype.hasOwnProperty, sample, ["present"]));
console.log("reflect_apply_max=" + Reflect.apply(Math.max, null, [2, 7]));

// 12) A getter borrowed out of a descriptor, called with a chosen receiver.
const source: any = {
  _v: "inner",
  get value(): string { return "read:" + this._v; },
};
const getValue: any = Function.prototype.call.bind(
  (Object.getOwnPropertyDescriptor(source, "value") as any).get,
);
console.log("borrowed_getter=" + getValue(source));
console.log("borrowed_getter_foreign=" + getValue({ _v: "foreign" }));

// 13) The idiom applied to `bind` itself produces a binder.
const binderOf: any = Function.prototype.call.bind(Function.prototype.bind);
function shout(word: string): string {
  const self: any = this;
  return (self && self.prefix ? self.prefix : "-") + word;
}
const boundShout: any = binderOf(shout, { prefix: ">>" });
console.log("binder_of=" + boundShout("hi"));

// 14) A helper that keeps working after its target gains properties.
(Object.prototype.hasOwnProperty as any).marker = "late";
console.log("helper_unaffected=" + hasOwn(sample, "present") + "|marker_on_target=" +
  (Object.prototype.hasOwnProperty as any).marker);

// 15) Borrowing a method that does not exist on the receiver's prototype chain
//     still works, because the receiver is only a `this`.
const push: any = Function.prototype.call.bind(Array.prototype.push);
const pseudoArray: any = { length: 0 };
push(pseudoArray, "one");
push(pseudoArray, "two");
console.log("borrowed_push=" + pseudoArray.length + "|" + pseudoArray[0] + "," + pseudoArray[1]);
console.log("borrowed_push_not_array=" + Array.isArray(pseudoArray));

// 16) `call` on a bound function: the receiver argument is ignored.
const alreadyBound = shout.bind({ prefix: "**" });
console.log("call_on_bound=" + Function.prototype.call.call(alreadyBound, { prefix: "??" }, "x"));

// 17) The helper's own `this` — it is a bound function, so calling it as a
//     method changes nothing.
const asMethod: any = { hasOwn };
console.log("as_method=" + asMethod.hasOwn(sample, "present"));
