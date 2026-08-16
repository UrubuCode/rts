// Pins what survives revocation: `typeof` still answers from the target's
// shape, everything that touches an internal method throws TypeError — even
// Array.isArray and Object.prototype.toString — and revoke() itself is
// idempotent and answers undefined.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

const objRev = Proxy.revocable({ a: 1 }, {});
const arrRev = Proxy.revocable([1, 2, 3], {});
const fnRev = Proxy.revocable(function f(x: number) { return x; }, {});

// the revocable result is a plain object with exactly two own keys
console.log("result_keys=" + Object.keys(objRev).sort().join("|"));
console.log("result_proto=" + (Object.getPrototypeOf(objRev) === Object.prototype));
console.log("revoke_kind=" + typeof objRev.revoke + ",len=" + (objRev.revoke as any).length + ",name=[" + (objRev.revoke as any).name + "]");
console.log("revocable_len=" + Proxy.revocable.length);

const p: any = objRev.proxy;
const pa: any = arrRev.proxy;
const pf: any = fnRev.proxy;

console.log("before_get=" + p.a);
console.log("before_isArray=" + Array.isArray(pa));
console.log("before_tag=" + Object.prototype.toString.call(pa));
console.log("before_call=" + pf(5));

// a child that inherits from the proxy, made while it still worked
const child: any = Object.create(p);
console.log("before_child=" + child.a);

console.log("revoke_returns=" + String(objRev.revoke()));
console.log("revoke_again=" + String(objRev.revoke()));
arrRev.revoke();
fnRev.revoke();

// typeof is the one question answered without an internal method
console.log("after_typeof_obj=" + typeof p);
console.log("after_typeof_fn=" + typeof pf);
console.log("after_typeof_arr=" + typeof pa);
console.log("after_identity=" + (p === objRev.proxy));

attempt("get", () => String(p.a));
attempt("set", () => String(Reflect.set(p, "a", 2)));
attempt("has", () => String("a" in p));
attempt("delete", () => String(Reflect.deleteProperty(p, "a")));
attempt("ownKeys", () => Reflect.ownKeys(p).join("|"));
attempt("keys", () => Object.keys(p).join("|"));
attempt("gopd", () => String(Object.getOwnPropertyDescriptor(p, "a")));
attempt("define", () => String(Reflect.defineProperty(p, "b", { value: 1 })));
attempt("getproto", () => String(Reflect.getPrototypeOf(p)));
attempt("setproto", () => String(Reflect.setPrototypeOf(p, null)));
attempt("isExtensible", () => String(Reflect.isExtensible(p)));
attempt("preventExtensions", () => String(Reflect.preventExtensions(p)));
attempt("json", () => String(JSON.stringify(p)));
attempt("spread", () => Object.keys({ ...p }).join("|"));
attempt("forin", () => { let n = 0; for (const _k in p) { n++; } return String(n); });
attempt("string", () => String(p));
attempt("isArray", () => String(Array.isArray(pa)));
attempt("tag", () => Object.prototype.toString.call(pa));
attempt("call", () => String(pf(5)));
attempt("construct", () => String(typeof new pf(1)));
attempt("instanceof_rhs", () => String({} instanceof pf));
attempt("instanceof_lhs", () => String(p instanceof Object));
attempt("via_child", () => String(child.a));
attempt("freeze", () => { Object.freeze(p); return "ok"; });

// a revoked proxy is still a legal TARGET for a new proxy — the failure comes
// at the first operation, not at construction
const wrapper: any = new Proxy(p, {});
console.log("wrap_built=" + typeof wrapper);
attempt("wrap_get", () => String(wrapper.a));
// and a legal HANDLER, with the same delayed failure
const handlerRev = Proxy.revocable({ get() { return "h"; } }, {});
const usesHandler: any = new Proxy({ a: 1 }, handlerRev.proxy as any);
console.log("handler_before=" + usesHandler.a);
handlerRev.revoke();
attempt("handler_after", () => String(usesHandler.a));
