// Pins the Proxy constructor as an object: it needs `new`, it refuses a
// non-object target or handler, it has NO prototype property (so `instanceof
// Proxy` throws), and the proxy it returns owns nothing of its own.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

console.log("typeof=" + typeof Proxy);
console.log("name=" + Proxy.name + ",length=" + Proxy.length);
console.log("own_names=" + Object.getOwnPropertyNames(Proxy).sort().join("|"));
console.log("has_prototype=" + ("prototype" in Proxy));
console.log("proto_of_Proxy=" + (Object.getPrototypeOf(Proxy) === Function.prototype));
const nameDesc = Object.getOwnPropertyDescriptor(Proxy, "name") as any;
console.log("name_desc=w=" + nameDesc.writable + ",e=" + nameDesc.enumerable + ",c=" + nameDesc.configurable);
console.log("globalThis_desc=" + (() => {
  const d = Object.getOwnPropertyDescriptor(globalThis as any, "Proxy") as any;
  return d === undefined ? "none" : "w=" + d.writable + ",e=" + d.enumerable + ",c=" + d.configurable;
})());

// calling without new is a TypeError
attempt("call_no_new", () => String((Proxy as any)({}, {})));
attempt("reflect_apply", () => String(Reflect.apply(Proxy as any, undefined, [{}, {}])));
console.log("reflect_construct=" + typeof Reflect.construct(Proxy as any, [{}, {}]));

// both arguments must be objects
attempt("null_target", () => String(new Proxy(null as any, {})));
attempt("undefined_target", () => String(new Proxy(undefined as any, {})));
attempt("number_target", () => String(new Proxy(1 as any, {})));
attempt("string_target", () => String(new Proxy("s" as any, {})));
attempt("symbol_target", () => String(new Proxy(Symbol("s") as any, {})));
attempt("null_handler", () => String(new Proxy({}, null as any)));
attempt("number_handler", () => String(new Proxy({}, 1 as any)));
attempt("missing_args", () => String(new (Proxy as any)()));
attempt("one_arg", () => String(new (Proxy as any)({})));
console.log("function_handler=" + typeof new Proxy({}, function () { /* callable objects are objects */ } as any));
console.log("array_handler=" + typeof new Proxy({}, [] as any));
console.log("extra_args_ignored=" + typeof new (Proxy as any)({ a: 1 }, {}, "ignored"));

// there is no Proxy.prototype, so instanceof has nothing to walk
const p: any = new Proxy({ a: 1 }, {});
attempt("instanceof_Proxy", () => String(p instanceof Proxy));
console.log("instanceof_Object=" + (p instanceof Object));
console.log("constructor_is_Object=" + (p.constructor === Object));
console.log("proto_is_Object_prototype=" + (Object.getPrototypeOf(p) === Object.prototype));

// a proxy owns nothing: everything visible belongs to the target
console.log("own_keys=" + Reflect.ownKeys(p).join("|"));
console.log("own_keys_empty_target=" + Reflect.ownKeys(new Proxy({}, {})).length);
console.log("is_extensible=" + Object.isExtensible(p));
console.log("identity=" + (new Proxy({}, {}) === new Proxy({}, {})));

// the target and handler are not reachable through the proxy
const secretTarget: any = { s: 1 };
const secretHandler: any = { get(t: any, k: any, r: any) { return Reflect.get(t, k, r); } };
const secretive: any = new Proxy(secretTarget, secretHandler);
console.log("no_target_prop=" + String(secretive.target) + ":" + String((secretive as any)["[[Target]]"]));
console.log("target_not_equal=" + (secretive === secretTarget));
console.log("handler_not_a_key=" + (Reflect.ownKeys(secretive).indexOf("handler") < 0));

// Proxy.revocable: same argument rules, a two-key result, and a fresh revoke
console.log("revocable_name=" + Proxy.revocable.name + ",length=" + Proxy.revocable.length);
attempt("revocable_no_new", () => typeof Proxy.revocable({}, {}));
attempt("revocable_with_new", () => String(new (Proxy.revocable as any)({}, {})));
attempt("revocable_bad_target", () => String(Proxy.revocable(5 as any, {})));
const r1 = Proxy.revocable({}, {});
const r2 = Proxy.revocable({}, {});
console.log("revoke_distinct=" + (r1.revoke !== r2.revoke));
console.log("revoke_proto=" + (Object.getPrototypeOf(r1.revoke) === Function.prototype));
console.log("result_desc=" + (() => {
  const d = Object.getOwnPropertyDescriptor(r1, "proxy") as any;
  return "w=" + d.writable + ",e=" + d.enumerable + ",c=" + d.configurable;
})());
