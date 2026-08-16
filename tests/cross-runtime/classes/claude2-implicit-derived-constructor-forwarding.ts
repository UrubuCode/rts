// Cross-runtime: a derived class with NO constructor gets the implicit
// `constructor(...args) { super(...args) }` — every argument is forwarded
// unchanged through as many implicit levels as the chain has, its `length` is
// 0, and `prototype.constructor` still names the derived class.
class A {
  args: string = "";
  count: number = 0;
  newTargetName: string = "";
  constructor(first: any, second: any, ...rest: any[]) {
    this.args = [first, second].concat(rest).map((v) => String(v)).join(",");
    this.count = arguments.length;
    this.newTargetName = new.target === undefined ? "none" : (new.target as any).name;
  }
}

// One implicit level.
class B extends A {}

// A level with fields but still no constructor.
class C extends B {
  tag: string = "c";
  seen: string = "seen:" + "c";
}

// Two more implicit levels on top.
class D extends C {}
class E extends D {}

const d = new D(1, 2, 3, 4);
console.log("d-args=" + d.args);
console.log("d-count=" + d.count);
console.log("d-newtarget=" + d.newTargetName);
console.log("d-tag=" + d.tag);
console.log("d-instanceof-a=" + (d instanceof A));

const e = new E("x", "y");
console.log("e-args=" + e.args);
console.log("e-count=" + e.count);
console.log("e-newtarget=" + e.newTargetName);

// Fewer arguments than the base declares: nothing is padded, arguments.length
// is what the caller passed.
const short = new E("only");
console.log("short-args=" + short.args);
console.log("short-count=" + short.count);

// Zero arguments still reaches the base.
const none = new B();
console.log("none-args=" + JSON.stringify(none.args));
console.log("none-count=" + none.count);
console.log("none-newtarget=" + none.newTargetName);

// The implicit constructor's own arity is 0 whatever the base declares.
console.log("a-length=" + A.length);
console.log("b-length=" + B.length);
console.log("c-length=" + C.length);
console.log("e-length=" + E.length);

// Each level's prototype carries a `constructor` naming that level, and
// nothing else.
console.log("b-proto-ctor=" + (B.prototype.constructor === B));
console.log("e-proto-ctor=" + (E.prototype.constructor === E));
console.log("b-proto-names=" + Object.getOwnPropertyNames(B.prototype).join(","));
console.log("c-proto-names=" + Object.getOwnPropertyNames(C.prototype).sort().join(","));

// The implicit constructor is a distinct function object per level.
console.log("distinct-bc=" + (B.prototype.constructor !== C.prototype.constructor));
console.log("distinct-ba=" + (B.prototype.constructor !== A));
console.log("b-ctor-name=" + B.prototype.constructor.name);

// Reflect.construct feeds the same forwarding path.
const r: any = Reflect.construct(E, ["p", "q", "r"]);
console.log("reflect-args=" + r.args);
console.log("reflect-count=" + r.count);
console.log("reflect-newtarget=" + r.newTargetName);

// An explicit newTarget changes only what the base observes, not the chain.
const r2: any = Reflect.construct(E, ["m"], C);
console.log("reflect2-args=" + r2.args);
console.log("reflect2-newtarget=" + r2.newTargetName);
console.log("reflect2-instanceof-c=" + (r2 instanceof C));
console.log("reflect2-instanceof-e=" + (r2 instanceof E));

// Spread at the call site is forwarded as separate arguments, not as an array.
const spreadArgs = [7, 8, 9];
const s = new D(...spreadArgs);
console.log("spread-args=" + s.args);
console.log("spread-count=" + s.count);
