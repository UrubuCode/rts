// Cross-runtime: the four function KINDS as objects — what `typeof` says, which
// intrinsic constructor sits behind each, which of them carry a `prototype`,
// and which can be called with `new`.

function plain(a: number): number { return a; }
const arrow = (a: number): number => a;
function* gen(a: number): Generator<number> { yield a; }
async function asyncFn(a: number): Promise<number> { return a; }
async function* asyncGen(a: number): AsyncGenerator<number> { yield a; }
const method = { m(a: number): number { return a; } }.m;
class Klass { constructor(a: number) { void a; } }

const kinds: Array<[string, any]> = [
  ["plain", plain],
  ["arrow", arrow],
  ["generator", gen],
  ["async", asyncFn],
  ["asyncGenerator", asyncGen],
  ["method", method],
  ["class", Klass],
];

// 1) Every one of them is `typeof "function"`.
console.log("typeof=" + kinds.map(([n, f]) => n + ":" + typeof f).join(" "));

// 2) The brand from Object.prototype.toString differs by kind.
console.log("tags=" + kinds.map(([n, f]) => n + ":" +
  Object.prototype.toString.call(f).slice(8, -1)).join(" "));

// 3) The constructor behind each is a distinct intrinsic, reached through the
//    function's prototype rather than by name.
function intrinsicName(f: any): string {
  const proto = Object.getPrototypeOf(f);
  return proto === Function.prototype ? "Function.prototype" : proto.constructor.name;
}
console.log("intrinsics=" + kinds.map(([n, f]) => n + ":" + intrinsicName(f)).join(" "));

// 4) Those intrinsics are not global names, but they are reachable.
const GeneratorFunction: any = Object.getPrototypeOf(gen).constructor;
const AsyncFunction: any = Object.getPrototypeOf(asyncFn).constructor;
console.log("generator_ctor_global=" + (typeof (globalThis as any).GeneratorFunction));
console.log("generator_ctor_reachable=" + GeneratorFunction.name + "/" + AsyncFunction.name);
console.log("intrinsic_is_function=" + (GeneratorFunction instanceof Function));

// 5) Which kinds own a `prototype` property.
console.log("has_prototype=" + kinds.map(([n, f]) =>
  n + ":" + Object.prototype.hasOwnProperty.call(f, "prototype")).join(" "));

// 6) The attributes of that property differ: a class's is fixed, a
//    declaration's and a generator's are writable.
function protoAttrs(f: any): string {
  const d: any = Object.getOwnPropertyDescriptor(f, "prototype");
  if (!d) return "none";
  return "w=" + d.writable + " e=" + d.enumerable + " c=" + d.configurable;
}
console.log("plain_prototype=" + protoAttrs(plain));
console.log("generator_prototype=" + protoAttrs(gen));
console.log("class_prototype=" + protoAttrs(Klass));
console.log("async_prototype=" + protoAttrs(asyncFn));

// 7) A generator's `prototype` has no `constructor` back-link, unlike a plain
//    function's.
console.log("plain_backlink=" + (plain.prototype.constructor === plain));
console.log("generator_backlink=" + Object.prototype.hasOwnProperty.call(gen.prototype, "constructor"));

// 8) Which kinds can be constructed.
function tryNew(f: any): string {
  try {
    const made = new f(1);
    return "ok:" + typeof made;
  } catch (e) {
    return "threw:" + (e as any).constructor.name;
  }
}
console.log("constructible=" + kinds.map(([n, f]) => n + ":" + tryNew(f)).join(" "));

// 9) Which kinds can be called without `new`.
function tryCall(f: any): string {
  try {
    const out = f(1);
    return "ok:" + (out && typeof out === "object" ? "object" : typeof out);
  } catch (e) {
    return "threw:" + (e as any).constructor.name;
  }
}
console.log("callable=" + kinds.map(([n, f]) => n + ":" + tryCall(f)).join(" "));

// 10) `name` and `length` are the same shape across kinds.
console.log("names=" + kinds.map(([n, f]) => n + ":" + JSON.stringify(f.name)).join(" "));
console.log("lengths=" + kinds.map(([n, f]) => n + ":" + f.length).join(" "));

// 11) Calling a generator produces an object that is both iterable and an
//     iterator, tagged as a Generator.
const it: any = gen(5);
console.log("generator_object_tag=" + Object.prototype.toString.call(it).slice(8, -1));
console.log("generator_self_iterable=" + (it[Symbol.iterator]() === it));
console.log("generator_first=" + JSON.stringify(it.next()));
console.log("generator_second=" + JSON.stringify(it.next()));

// 12) The generator object's prototype chain starts at the function's own
//     `prototype` object.
console.log("generator_proto_link=" + (Object.getPrototypeOf(it) === gen.prototype));
console.log("generator_proto_has_next=" + ("next" in Object.getPrototypeOf(Object.getPrototypeOf(it))));

// 13) Calling an async function returns a promise immediately, without running
//     to completion first.
const promise: any = asyncFn(3);
console.log("async_returns=" + (promise instanceof Promise) + "/" + promise.constructor.name);
console.log("async_tag=" + Object.prototype.toString.call(promise).slice(8, -1));

// 14) An async generator's object is tagged differently and answers a promise
//     from `next`.
const agen: any = asyncGen(1);
console.log("async_generator_tag=" + Object.prototype.toString.call(agen).slice(8, -1));
console.log("async_generator_next_is_promise=" + (agen.next() instanceof Promise));
console.log("async_generator_has_asyncIterator=" + (typeof agen[Symbol.asyncIterator] === "function"));

// 15) A generator method in a class body is a generator function too.
class WithGen {
  *items(): Generator<number> { yield 1; }
  async load(): Promise<number> { return 1; }
}
console.log("class_gen_method=" + intrinsicName(WithGen.prototype.items));
console.log("class_async_method=" + intrinsicName(WithGen.prototype.load));
console.log("class_gen_method_tag=" +
  Object.prototype.toString.call(WithGen.prototype.items).slice(8, -1));

// 16) A class is a function whose prototype chain reaches Function.prototype,
//     and a subclass's reaches its parent.
class SubKlass extends Klass {}
console.log("class_proto=" + (Object.getPrototypeOf(Klass) === Function.prototype));
console.log("subclass_proto=" + (Object.getPrototypeOf(SubKlass) === Klass));
console.log("class_instanceof_function=" + (Klass instanceof Function));

// 17) Each intrinsic prototype carries its own Symbol.toStringTag.
console.log("generator_fn_tag=" + (Object.getPrototypeOf(gen) as any)[Symbol.toStringTag]);
console.log("async_fn_tag=" + (Object.getPrototypeOf(asyncFn) as any)[Symbol.toStringTag]);
