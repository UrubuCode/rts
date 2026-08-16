// Cross-runtime: how `new` binds against member access and call arguments.
// `new a.b.C()` news the deepest member; `new C().m()` calls on the instance;
// `new C` without an argument list is legal and takes no arguments.

function Point(this: any, x: number, y: number) {
  this.x = x === undefined ? -1 : x;
  this.y = y === undefined ? -1 : y;
}
Point.prototype.sum = function (this: any) { return this.x + this.y; };
Point.prototype.label = "pt";

const ns: any = { deep: { Point: Point } };

// Member access binds tighter than `new`: the whole path is the constructor.
const p1 = new ns.deep.Point(1, 2);
console.log("path_ctor=" + p1.sum());
console.log("path_is_point=" + (p1 instanceof (Point as any)));

// No argument list at all.
const p2 = new (Point as any)();
const p3 = new (ns.deep.Point as any)();
console.log("no_args=" + p2.x + "," + p2.y);
console.log("no_args_path=" + p3.x + "," + p3.y);

// `new C().m()` — the call after the argument list applies to the instance.
console.log("new_then_call=" + new (Point as any)(3, 4).sum());
console.log("new_then_prop=" + new (Point as any)(3, 4).label);

// Chaining a member off a parenthesised `new` with no arguments.
console.log("no_arg_then_prop=" + (new (Point as any)()).label);

// A constructor that RETURNS an object overrides the fresh `this`.
function Maker(this: any) {
  this.marker = "ignored";
  return Point;
}
const made = new (Maker as any)();
console.log("returns_object=" + (made === Point));
console.log("marker_lost=" + (made.marker === undefined));

// So `new new Maker()(5, 6)` news whatever Maker returned.
const doubleNew = new (new (Maker as any)())(5, 6);
console.log("double_new=" + doubleNew.sum());
console.log("double_new_is_point=" + (doubleNew instanceof (Point as any)));

// A constructor returning a PRIMITIVE does not override `this`.
function Primitive(this: any) {
  this.kept = 7;
  return 42 as any;
}
const prim = new (Primitive as any)();
console.log("primitive_return_ignored=" + prim.kept);

// `new (f())()` — parentheses force the call first.
function factory(): any { return Point; }
const viaFactory = new (factory())(8, 9);
console.log("via_factory=" + viaFactory.sum());

// Without parentheses, `new factory()` news the factory itself.
const newedFactory: any = new (factory as any)();
console.log("newed_factory_is_point=" + (newedFactory === Point));

// A computed member in the constructor path.
const key = "Point";
const viaComputed = new ns.deep[key](10, 20);
console.log("computed_path=" + viaComputed.sum());

// `typeof` over a `new` expression.
console.log("typeof_new=" + typeof new (Point as any)(0, 0));

// Argument expressions are evaluated left to right, after the callee.
const order: string[] = [];
function note(tagName: string, value: number): number { order.push(tagName); return value; }
function callee(): any { order.push("callee"); return Point; }
new (callee())(note("a", 1), note("b", 2));
console.log("eval_order=" + order.join(","));

// A class as the constructor behind a member path.
class Boxed {
  v: number;
  constructor(v: number) { this.v = v; }
  double(): number { return this.v * 2; }
  static make(v: number): Boxed { return new Boxed(v); }
}
const holder: any = { Boxed: Boxed };
console.log("class_path=" + new holder.Boxed(21).double());
console.log("static_returns=" + Boxed.make(5).double());

// `new` on the result of a static that returns the class itself.
const Self: any = { get(): any { return Boxed; } };
console.log("new_from_getter_fn=" + new (Self.get())(3).double());

// instanceof binds looser than `new`.
console.log("instanceof_binding=" + (new (Point as any)(1, 1) instanceof (Point as any)));

// A `new` expression as an argument to another `new`.
function Wrapper(this: any, inner: any) { this.inner = inner; }
const wrapped = new (Wrapper as any)(new (Point as any)(2, 3));
console.log("nested_new_arg=" + wrapped.inner.sum());

// Optional chaining cannot follow `new` directly, but can follow the instance.
const maybe: any = new (Point as any)(4, 4);
console.log("optional_after_new=" + maybe?.sum());
console.log("optional_missing=" + maybe?.nothere?.deep);
