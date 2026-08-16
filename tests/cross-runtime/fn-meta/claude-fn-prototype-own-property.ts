// Cross-runtime: which callables carry an own `prototype` property, and with
// which attributes. Only the constructible ones do — arrows, shorthand methods,
// accessors and bound functions do not.

function decl(): void {}
const expr = function (): void {};
const arrow = (): void => {};
const shorthand = { m(): void {} }.m;
const getterFn = (Object.getOwnPropertyDescriptor({ get g(): number { return 1; } }, "g") as any).get;
function* gen(): Generator<number> { yield 1; }
async function asyncFn(): Promise<void> { /* nothing */ }
async function* asyncGen(): AsyncGenerator<number> { yield 1; }
class Klass { m(): void {} }
const bound = decl.bind(null);

function has(fn: any): string {
  return Object.prototype.hasOwnProperty.call(fn, "prototype") ? "yes" : "no";
}

console.log("decl=" + has(decl));
console.log("expr=" + has(expr));
console.log("arrow=" + has(arrow));
console.log("shorthand_method=" + has(shorthand));
console.log("getter=" + has(getterFn));
console.log("generator=" + has(gen));
console.log("async=" + has(asyncFn));
console.log("async_generator=" + has(asyncGen));
console.log("class=" + has(Klass));
console.log("class_method=" + has(Klass.prototype.m));
console.log("bound=" + has(bound));
console.log("arrow_prototype_value=" + String((arrow as any).prototype));

// Attributes of `prototype` differ between a function and a class.
function attrs(fn: any): string {
  const d = Object.getOwnPropertyDescriptor(fn, "prototype");
  if (d === undefined) return "absent";
  return (d as any).writable + "/" + (d as any).enumerable + "/" + (d as any).configurable;
}
console.log("decl_attrs=" + attrs(decl));
console.log("gen_attrs=" + attrs(gen));
console.log("class_attrs=" + attrs(Klass));

// A function's prototype links back with a `constructor` property.
console.log("ctor_link=" + (decl.prototype.constructor === decl));
console.log("ctor_enumerable=" + (Object.getOwnPropertyDescriptor(decl.prototype, "constructor") as any).enumerable);
console.log("class_ctor_link=" + (Klass.prototype.constructor === Klass));
console.log("class_ctor_enumerable=" + (Object.getOwnPropertyDescriptor(Klass.prototype, "constructor") as any).enumerable);

// A generator's prototype has NO constructor and is not a plain object.
console.log("gen_proto_has_ctor=" + Object.prototype.hasOwnProperty.call(gen.prototype, "constructor"));
console.log("gen_proto_keys=" + Object.getOwnPropertyNames(gen.prototype).join(","));
console.log("gen_proto_proto_is_object=" + (Object.getPrototypeOf(gen.prototype) === Object.prototype));

// A function's prototype IS a plain object with Object.prototype behind it.
console.log("decl_proto_is_object=" + (Object.getPrototypeOf(decl.prototype) === Object.prototype));
console.log("decl_proto_keys=" + Object.getOwnPropertyNames(decl.prototype).join(","));

// Constructibility follows the same split.
function constructible(fn: any): string {
  try {
    new fn();
    return "yes";
  } catch (e) {
    return "no:" + (e as any).constructor.name;
  }
}
console.log("new_decl=" + constructible(decl));
console.log("new_arrow=" + constructible(arrow));
console.log("new_shorthand=" + constructible(shorthand));
console.log("new_generator=" + constructible(gen));
console.log("new_async=" + constructible(asyncFn));
console.log("new_class=" + constructible(Klass));
console.log("new_class_method=" + constructible(Klass.prototype.m));

// `prototype` is writable on a function, so it can be replaced wholesale.
const replacement = { marker: "swapped" };
(decl as any).prototype = replacement;
console.log("replaced=" + ((decl as any).prototype === replacement));
console.log("instance_sees_replacement=" + (new (decl as any)() as any).marker);

// A class's `prototype` is NOT writable, so the same write is refused.
// `Reflect.set` reports the refusal as a boolean in both strict and sloppy
// code, where a bare assignment only throws in strict.
const beforeWrite = Klass.prototype;
console.log("class_set_refused=" + Reflect.set(Klass, "prototype", {}));
console.log("class_define_refused=" + Reflect.defineProperty(Klass, "prototype", { value: {} }));
console.log("class_delete_refused=" + Reflect.deleteProperty(Klass, "prototype"));
console.log("class_prototype_unchanged=" + (Klass.prototype === beforeWrite));
console.log("class_prototype_intact=" + (typeof Klass.prototype.m === "function"));
console.log("class_prototype_writable=" + (Object.getOwnPropertyDescriptor(Klass, "prototype") as any).writable);

// Object.setPrototypeOf on an instance still reads through the chain.
const inst: any = new (function Base(this: any) { this.own = 1; } as any)();
console.log("own_property=" + inst.own);
console.log("proto_chain_ends=" + (Object.getPrototypeOf(Object.getPrototypeOf(inst)) === Object.prototype));

// The [[Prototype]] of the callables themselves.
console.log("decl_proto=" + (Object.getPrototypeOf(decl) === Function.prototype));
console.log("arrow_proto=" + (Object.getPrototypeOf(arrow) === Function.prototype));
console.log("class_proto=" + (Object.getPrototypeOf(Klass) === Function.prototype));
console.log("gen_proto_not_function=" + (Object.getPrototypeOf(gen) === Function.prototype));
