// Cross-runtime: a class body is always strict, so a method detached from its
// instance sees `this === undefined` and a primitive receiver is never boxed —
// while an arrow stored in a field captured `this` at construction and ignores
// every later rebinding.

class Widget {
  tag = "widget";
  // A method: `this` comes from the call site.
  read(): string { return this === undefined ? "lost" : (this as any).tag; }
  // An arrow field: `this` was captured when the instance was built.
  readArrow = (): string => (this === undefined ? "lost" : this.tag);
  // What kind of value the receiver is.
  kindOfThis(): string { return this === undefined ? "undefined" : typeof this; }
  // Identity of the receiver.
  isSame(other: any): boolean { return (this as any) === other; }
  static staticRead(): string { return this === undefined ? "lost" : (this as any).name; }
}

const w = new Widget();

// Called as a method, both work.
console.log("method=" + w.read());
console.log("arrow_field=" + w.readArrow());

// Detached, the method loses the receiver and the arrow does not.
const detachedMethod = w.read;
const detachedArrow = w.readArrow;
console.log("detached_method=" + detachedMethod());
console.log("detached_arrow=" + detachedArrow());

// Passed as a callback, the same split.
function invoke(fn: () => string): string { return fn(); }
console.log("callback_method=" + invoke(w.read));
console.log("callback_arrow=" + invoke(w.readArrow));
console.log("callback_bound=" + invoke(w.read.bind(w)));
console.log("callback_wrapped=" + invoke(() => w.read()));

// call/apply/bind steer the method and are ignored by the arrow.
const other = { tag: "other" };
console.log("method_call=" + detachedMethod.call(other));
console.log("method_apply=" + detachedMethod.apply(other));
console.log("method_bind=" + detachedMethod.bind(other)());
console.log("arrow_call=" + detachedArrow.call(other as any));
console.log("arrow_apply=" + detachedArrow.apply(other as any));
console.log("arrow_bind=" + detachedArrow.bind(other as any)());

// A primitive receiver reaches strict class code unboxed.
console.log("this_undefined=" + w.kindOfThis.call(undefined));
console.log("this_null=" + (w.kindOfThis.call(null) === "object" ? "object" : w.kindOfThis.call(null)));
console.log("this_number=" + w.kindOfThis.call(42));
console.log("this_string=" + w.kindOfThis.call("txt"));
console.log("this_boolean=" + w.kindOfThis.call(true));
console.log("this_symbol=" + w.kindOfThis.call(Symbol("s")));
console.log("this_bigint=" + w.kindOfThis.call(10n));
console.log("this_object=" + w.kindOfThis.call({}));

// And it is the very same primitive, not a copy in a wrapper.
const str = "shared";
console.log("identity_string=" + w.isSame.call(str, str));
console.log("identity_number=" + w.isSame.call(7, 7));
const sym = Symbol("id");
console.log("identity_symbol=" + w.isSame.call(sym, sym));
console.log("identity_object=" + w.isSame.call(other, other));

// A static method's `this` is the class when called on it.
console.log("static_on_class=" + Widget.staticRead());
const detachedStatic = Widget.staticRead;
console.log("static_detached=" + detachedStatic());

// Each instance gets its OWN arrow field; the method is shared on the prototype.
const w2 = new Widget();
w2.tag = "second";
console.log("arrow_per_instance=" + (w.readArrow !== w2.readArrow));
console.log("method_shared=" + (w.read === w2.read));
console.log("method_on_prototype=" + Object.prototype.hasOwnProperty.call(Widget.prototype, "read"));
console.log("arrow_on_instance=" + Object.prototype.hasOwnProperty.call(w, "readArrow"));
console.log("arrow_on_prototype=" + Object.prototype.hasOwnProperty.call(Widget.prototype, "readArrow"));
console.log("second_arrow=" + w2.readArrow());

// The arrow field's captured `this` follows the instance even when moved.
const moved = { tag: "moved", readArrow: w.readArrow };
console.log("moved_arrow=" + moved.readArrow());

// A method on the prototype is enumerable? No — class methods are not.
const md = Object.getOwnPropertyDescriptor(Widget.prototype, "read") as any;
console.log("method_enumerable=" + md.enumerable);
console.log("method_writable=" + md.writable);
const ad = Object.getOwnPropertyDescriptor(w, "readArrow") as any;
console.log("arrow_field_enumerable=" + ad.enumerable);
console.log("arrow_field_writable=" + ad.writable);

// Subclass: the inherited method still takes `this` from the call site.
class Sub extends Widget {
  tag = "sub";
  describe(): string { return this.read() + "/" + this.readArrow(); }
}
const s = new Sub();
console.log("subclass=" + s.describe());
console.log("subclass_detached=" + invoke(s.read));
console.log("subclass_arrow=" + invoke(s.readArrow));
console.log("subclass_method_is_inherited=" + (s.read === Widget.prototype.read));

// An arrow declared in a method captures that call's `this`.
class Later {
  tag = "later";
  defer(): () => string { return () => this.tag; }
}
const deferred = new Later().defer();
console.log("deferred_arrow=" + deferred());
console.log("deferred_rebind_ignored=" + deferred.call({ tag: "nope" } as any));
