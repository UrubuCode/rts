// Cross-runtime: what `bind` does to a function's METADATA — `name` gains a
// "bound " prefix once per bind, `length` drops by the number of bound
// arguments and never goes below 0, and the bound function has no `prototype`.

function three(a: number, b: number, c: number): number { return a + b + c; }

console.log("orig_name=" + three.name);
console.log("orig_length=" + three.length);

const b0 = three.bind(null);
console.log("b0_name=" + b0.name);
console.log("b0_length=" + b0.length);

const b1 = three.bind(null, 1);
console.log("b1_name=" + b1.name);
console.log("b1_length=" + b1.length);

const b2 = three.bind(null, 1, 2);
console.log("b2_length=" + b2.length);

const b3 = three.bind(null, 1, 2, 3);
console.log("b3_length=" + b3.length);

// More bound arguments than parameters clamps the length at 0.
const b5 = (three as any).bind(null, 1, 2, 3, 4, 5);
console.log("b5_length=" + b5.length);
console.log("b5_result=" + b5());

// Binding a bound function stacks the prefix and keeps subtracting.
const twice = b1.bind(null, 2);
console.log("twice_name=" + twice.name);
console.log("twice_length=" + twice.length);
const thrice = twice.bind(null, 3);
console.log("thrice_name=" + thrice.name);
console.log("thrice_length=" + thrice.length);
console.log("thrice_result=" + thrice());

// An anonymous function keeps its inferred name through bind.
const anon = function (): number { return 1; };
console.log("anon_name=" + anon.name);
console.log("anon_bound_name=" + anon.bind(null).name);

// A truly nameless function binds to "bound ".
const nameless = (function (): any { return function (): number { return 0; }; })();
console.log("nameless_name=" + JSON.stringify(nameless.name));
console.log("nameless_bound=" + JSON.stringify(nameless.bind(null).name));

// An arrow binds like any other function.
const arrow = (x: number, y: number): number => x * y;
console.log("arrow_name=" + arrow.name);
console.log("arrow_bound_name=" + arrow.bind(null).name);
console.log("arrow_bound_length=" + arrow.bind(null, 2).length);

// Defaults and rest already cut `length`; bind cuts what is left.
function withDefault(a: number, b: number = 2, c?: number): number { return a + b + (c ?? 0); }
console.log("default_length=" + withDefault.length);
console.log("default_bound_length=" + withDefault.bind(null, 1).length);
console.log("default_bound_twice=" + withDefault.bind(null, 1, 2).length);

function withRest(a: number, ...rest: number[]): number { return a + rest.length; }
console.log("rest_length=" + withRest.length);
console.log("rest_bound_length=" + withRest.bind(null, 1).length);

// `name` and `length` on a bound function are configurable, not writable.
const nd = Object.getOwnPropertyDescriptor(b1, "name") as any;
console.log("name_writable=" + nd.writable);
console.log("name_enumerable=" + nd.enumerable);
console.log("name_configurable=" + nd.configurable);
const ld = Object.getOwnPropertyDescriptor(b1, "length") as any;
console.log("length_writable=" + ld.writable);
console.log("length_configurable=" + ld.configurable);

// The bound function has no own `prototype`, unlike the target.
console.log("target_has_prototype=" + Object.prototype.hasOwnProperty.call(three, "prototype"));
console.log("bound_has_prototype=" + Object.prototype.hasOwnProperty.call(b1, "prototype"));
console.log("bound_prototype_value=" + String((b1 as any).prototype));

// Its own keys are exactly length and name.
console.log("bound_own_keys=" + Object.getOwnPropertyNames(b1).sort().join(","));

// The bound function's [[Prototype]] is the target's.
console.log("proto_is_target_proto=" + (Object.getPrototypeOf(b1) === Object.getPrototypeOf(three)));
console.log("proto_is_Function=" + (Object.getPrototypeOf(b1) === Function.prototype));

// Binding a method keeps the receiver whatever later call/apply says.
const holder = { base: 100, take(this: any, n: number): number { return this.base + n; } };
const takeBound = holder.take.bind({ base: 5 });
console.log("bound_receiver=" + takeBound(1));
console.log("call_cannot_rebind=" + takeBound.call({ base: 999 }, 1));
console.log("apply_cannot_rebind=" + takeBound.apply({ base: 999 }, [1] as any));
console.log("rebind_cannot_rebind=" + takeBound.bind({ base: 777 })(1));

// A class method bound and stored still reports the method name.
class Counter {
  n = 3;
  read(this: any): number { return this.n; }
}
const c = new Counter();
console.log("method_bound_name=" + c.read.bind(c).name);
console.log("method_bound_value=" + c.read.bind(c)());
