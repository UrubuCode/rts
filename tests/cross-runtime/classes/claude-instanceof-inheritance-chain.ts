// Cross-runtime: instanceof along a multi-level inheritance chain.
class A {}
class B extends A {}
class C extends B {}
class Other {}

const a = new A();
const b = new B();
const c = new C();
const o = new Other();

console.log("a_A=" + (a instanceof A));
console.log("a_B=" + (a instanceof B));
console.log("a_C=" + (a instanceof C));

console.log("b_A=" + (b instanceof A));
console.log("b_B=" + (b instanceof B));
console.log("b_C=" + (b instanceof C));

console.log("c_A=" + (c instanceof A));
console.log("c_B=" + (c instanceof B));
console.log("c_C=" + (c instanceof C));

console.log("o_A=" + (o instanceof A));
console.log("o_Other=" + (o instanceof Other));

// everything is an Object; nothing is a Function
console.log("c_Object=" + (c instanceof Object));
console.log("c_Function=" + (c instanceof Function));
console.log("ctor_Function=" + (C instanceof Function));

// primitives and null are never instances
console.log("num_A=" + ((5 as unknown) instanceof A));
console.log("str_Object=" + (("s" as unknown) instanceof Object));
console.log("null_Object=" + ((null as unknown) instanceof Object));

// constructor property walks up to the nearest definition
console.log("c_ctor_name=" + c.constructor.name);
console.log("b_ctor_name=" + b.constructor.name);
console.log("c_ctor_is_C=" + (c.constructor === C));

// prototype chain relations
console.log("proto_B_of_C=" + (Object.getPrototypeOf(C.prototype) === B.prototype));
console.log("isProto_A_c=" + A.prototype.isPrototypeOf(c));
console.log("isProto_C_b=" + C.prototype.isPrototypeOf(b));
console.log("static_inherit=" + (Object.getPrototypeOf(C) === B));

// instanceof narrowing while sweeping a mixed array
const items: object[] = [a, b, c, o];
const marks: string[] = [];
for (let i = 0; i < items.length; i++) {
  const x = items[i];
  if (x instanceof C) marks.push("C");
  else if (x instanceof B) marks.push("B");
  else if (x instanceof A) marks.push("A");
  else marks.push("?");
}
console.log("sweep=" + marks.join(","));

// counting by base class
let countA = 0;
for (let i = 0; i < items.length; i++) {
  if (items[i] instanceof A) countA++;
}
console.log("count_A=" + countA);

// Error subclass chain
class MyErr extends Error {}
class SubErr extends MyErr {}
const e = new SubErr("boom");
console.log("e_SubErr=" + (e instanceof SubErr));
console.log("e_MyErr=" + (e instanceof MyErr));
console.log("e_Error=" + (e instanceof Error));
console.log("e_msg=" + e.message);
