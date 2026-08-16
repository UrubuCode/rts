// Pins what a proxy INHERITS from its target's shape rather than its handler:
// typeof answers "function" only for a callable target, [[Construct]] exists
// only if the target has one, and Array.isArray pierces the proxy. 380 covers
// the apply/construct traps but not these structural questions.

function plain(a: number, b: number): number { return a + b; }
const arrow = (x: number) => x * 2;
class Klass { constructor(v: number) { (this as any).v = v; } }
const obj = { a: 1 };

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

const pPlain: any = new Proxy(plain, {});
const pArrow: any = new Proxy(arrow, {});
const pKlass: any = new Proxy(Klass, {});
const pObj: any = new Proxy(obj, {});

console.log("typeof_plain=" + typeof pPlain);
console.log("typeof_arrow=" + typeof pArrow);
console.log("typeof_class=" + typeof pKlass);
console.log("typeof_obj=" + typeof pObj);

console.log("call_plain=" + pPlain(2, 3));
console.log("call_arrow=" + pArrow(4));
attempt("call_obj", () => String(pObj()));

attempt("new_plain", () => "ok:" + typeof new pPlain(1, 2));
attempt("new_arrow", () => "ok:" + typeof new pArrow(1));
attempt("new_class", () => "v=" + new pKlass(9).v);
attempt("class_no_new", () => String(pKlass(1)));

// length/name are read through to the target
console.log("meta_plain=" + pPlain.length + ":" + pPlain.name);
console.log("meta_arrow=" + pArrow.length + ":" + pArrow.name);
console.log("meta_class=" + pKlass.length + ":" + pKlass.name);

// instanceof works through the proxy on either side
const inst = new Klass(1);
console.log("inst_of_proxy=" + (inst instanceof pKlass));
console.log("proxy_inst_of_class=" + (new pKlass(1) instanceof Klass));

// Array.isArray follows the proxy chain to the target
const arr = [1, 2, 3];
const pArr: any = new Proxy(arr, {});
const pArrArr: any = new Proxy(pArr, {});
console.log("isarray_arr=" + Array.isArray(pArr));
console.log("isarray_nested=" + Array.isArray(pArrArr));
console.log("isarray_obj=" + Array.isArray(pObj));
console.log("arr_tag=" + Object.prototype.toString.call(pArr));
console.log("fn_tag=" + Object.prototype.toString.call(pPlain));
console.log("obj_tag=" + Object.prototype.toString.call(pObj));

// a proxy of an array keeps the exotic length behaviour
pArr.push(4);
console.log("push=" + arr.join("|") + ",len=" + pArr.length);
pArr.length = 2;
console.log("truncate=" + arr.join("|"));
console.log("json_arr=" + JSON.stringify(pArr));
console.log("concat_spreads=" + JSON.stringify([0].concat(pArr)));

// a proxy whose HANDLER is itself a proxy: the handler lookup is trapped
const handlerReads: string[] = [];
const handler = new Proxy({} as any, {
  get(_t, k) { handlerReads.push(String(k)); return undefined; },
});
const doubleProxy: any = new Proxy({ z: 1 }, handler);
console.log("double_read=" + doubleProxy.z);
console.log("double_keys=" + Object.keys(doubleProxy).join("|"));
console.log("handler_reads=" + handlerReads.join("|"));

// a proxy of a proxy of a function stays callable and constructable
const doubleFn: any = new Proxy(new Proxy(plain, {}), {});
console.log("double_fn=" + typeof doubleFn + ":" + doubleFn(1, 1));

// a callable proxy is accepted where a function is required
console.log("map_with_proxy=" + [1, 2, 3].map(pArrow as any).join("|"));
console.log("bind_through=" + (pPlain as any).bind(null, 10)(5));
console.log("apply_through=" + Reflect.apply(pPlain, null, [7, 8]));
attempt("construct_obj", () => String(Reflect.construct(pObj, [])));
console.log("construct_class=" + Reflect.construct(pKlass, [3]).v);

// a revoked proxy of a function loses even typeof-independent operations
const rv = Proxy.revocable(plain, {});
console.log("revoked_typeof_before=" + typeof rv.proxy);
rv.revoke();
console.log("revoked_typeof_after=" + typeof rv.proxy);
attempt("revoked_call", () => String((rv.proxy as any)(1, 2)));
attempt("revoked_keys", () => Object.keys(rv.proxy).join("|"));
