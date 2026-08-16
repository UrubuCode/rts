// Cross-runtime: WHERE an anonymous function picks a name up and where it does
// not. A binding, a default and a property in a literal all name it; an
// assignment to a member, a parenthesised expression, an argument and a return
// value all leave it empty.

function nameOf(fn: any): string {
  return JSON.stringify(fn.name);
}

// 1) A declaration binding names it; so does a plain assignment to an
//    identifier that already exists.
const fromConst = function (): void {};
let fromAssign: any;
fromAssign = () => {};
console.log("const_binding=" + nameOf(fromConst));
console.log("identifier_assign=" + nameOf(fromAssign));

// 2) A MEMBER target does not — the property name is not the function's.
const holder: any = {};
holder.member = function (): void {};
const list: any[] = [];
list[0] = () => {};
console.log("member_assign=" + nameOf(holder.member));
console.log("index_assign=" + nameOf(list[0]));

// 3) Wrapping in parentheses or a comma expression breaks the inference.
console.log("parenthesised=" + nameOf((function (): void {})));
console.log("comma_expression=" + nameOf((0, function (): void {})));

// 4) Logical assignment operators DO name it, because their target is an
//    identifier reference like a plain assignment's.
let orTarget: any = null;
orTarget ||= () => {};
let nullishTarget: any = null;
nullishTarget ??= function (): void {};
let andTarget: any = 1;
andTarget &&= () => {};
console.log("or_assign=" + nameOf(orTarget));
console.log("nullish_assign=" + nameOf(nullishTarget));
console.log("and_assign=" + nameOf(andTarget));

// 5) Destructuring defaults take the bound name, in both pattern shapes.
const [arrayDefault = () => {}] = [];
const { objectDefault = function (): void {} }: any = {};
console.log("array_default=" + nameOf(arrayDefault));
console.log("object_default=" + nameOf(objectDefault));

// 6) A parameter default takes the parameter's name.
function withDefault(callback: any = () => {}): string {
  return nameOf(callback);
}
console.log("param_default=" + withDefault());
console.log("param_supplied=" + withDefault(function given(): void {}));

// 7) A property in a literal names it, including a computed key whose value is
//    only known at run time.
const key = "computed";
const literal: any = {
  plain: function (): void {},
  arrow: () => {},
  method(): void {},
  [key]: () => {},
  ["con" + "cat"]: function (): void {},
};
console.log("literal_plain=" + nameOf(literal.plain));
console.log("literal_arrow=" + nameOf(literal.arrow));
console.log("literal_method=" + nameOf(literal.method));
console.log("literal_computed=" + nameOf(literal.computed));
console.log("literal_concat=" + nameOf(literal.concat));

// 8) A symbol key gives "[description]", and a description-less symbol gives
//    the empty string.
const described = Symbol("tagname");
const bare = Symbol();
const symbolKeyed: any = { [described]: () => {}, [bare]: () => {} };
console.log("symbol_described=" + nameOf(symbolKeyed[described]));
console.log("symbol_bare=" + nameOf(symbolKeyed[bare]));

// 9) A value that merely PASSES through a call or a return keeps nothing.
function returnsAnonymous(): any {
  return () => {};
}
function identity(f: any): any {
  return f;
}
console.log("returned=" + nameOf(returnsAnonymous()));
console.log("passed_as_argument=" + nameOf(identity(() => {})));

// 10) A class expression follows the same rules, and its own name wins.
const AnonClass = class {};
const NamedClass = class Inner {};
console.log("class_expression=" + nameOf(AnonClass));
console.log("class_expression_named=" + nameOf(NamedClass));
console.log("anonymous_instance_ctor=" + nameOf(new (class {})().constructor));

// 11) An explicit name on a function expression always wins over the binding.
const explicitWins = function theRealName(): void {};
console.log("explicit_wins=" + nameOf(explicitWins));

// 12) Generators and async functions infer exactly like plain ones.
const genFromBinding = function* (): Generator<number> { yield 1; };
const asyncFromBinding = async function (): Promise<void> { /* nothing */ };
console.log("generator_binding=" + nameOf(genFromBinding));
console.log("async_binding=" + nameOf(asyncFromBinding));

// 13) The name is fixed once, at creation: moving or copying the function later
//     changes nothing.
let moved: any = function (): void {};
const copy = moved;
moved = null;
console.log("copy_keeps_name=" + nameOf(copy));
const reHomed: any = { elsewhere: copy };
console.log("rehomed_keeps_name=" + nameOf(reHomed.elsewhere));

// 14) Re-assigning a NAMED binding does not rename the old function.
let renamed: any = function (): void {};
const firstOne = renamed;
renamed = function (): void {};
console.log("first_name=" + nameOf(firstOne) + "|second_name=" + nameOf(renamed));

// 15) A function created inside a descriptor literal takes the descriptor
//     field's name, which is `value` — the property name never reaches it.
const defined: any = {};
Object.defineProperty(defined, "prop", { value: () => {}, configurable: true });
console.log("define_property=" + nameOf(defined.prop));

// 16) An accessor pair in a literal carries the get/set prefix.
const accessors: any = { get thing(): number { return 1; }, set thing(v: number) { void v; } };
const desc: any = Object.getOwnPropertyDescriptor(accessors, "thing");
console.log("accessor_get=" + nameOf(desc.get) + "|accessor_set=" + nameOf(desc.set));

// 17) A nested literal names the innermost binding position, not the path.
const nested: any = { outer: { inner: () => {} } };
console.log("nested_literal=" + nameOf(nested.outer.inner));

// 18) A bound function prefixes whatever name the target had, including none.
console.log("bound_named=" + nameOf(fromConst.bind(null)));
console.log("bound_anonymous=" + nameOf((0, function (): void {}).bind(null)));

// 19) An IIFE's function is never named, and its RESULT is not a function.
const iifeResult = (function (): string { return "ran"; })();
console.log("iife_result=" + iifeResult);
console.log("iife_fn_name=" + nameOf(function (): void {}));

// 20) A class METHOD and a static method take their key; the constructor takes
//     the class's name.
class Shapes {
  instanceMethod(): void {}
  static staticMethod(): void {}
}
console.log("instance_method=" + nameOf(Shapes.prototype.instanceMethod));
console.log("static_method=" + nameOf(Shapes.staticMethod));
console.log("constructor_name=" + nameOf(Shapes.prototype.constructor));
