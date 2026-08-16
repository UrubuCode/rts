// Pins the two function traps: a proxy has [[Call]]/[[Construct]] only if the
// TARGET does — a trap on a plain object is dead code — the construct trap must
// return an object, and the apply trap is free to return anything.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

// a construct trap on a non-constructable target is never reached
let reached = "no";
const arrowProxy: any = new Proxy(() => 1, { construct() { reached = "yes"; return {}; } });
attempt("arrow_new", () => String(typeof new arrowProxy()));
console.log("arrow_trap_reached=" + reached);
console.log("arrow_call=" + arrowProxy());

const methodHost = { m() { return "M"; } };
const methodProxy: any = new Proxy(methodHost.m, { construct() { return {}; } });
attempt("method_new", () => String(typeof new methodProxy()));
console.log("method_call=" + methodProxy());

// an apply trap on a non-callable target is likewise unreachable
let applyReached = "no";
const objProxy: any = new Proxy({} as any, { apply() { applyReached = "yes"; return 1; } });
attempt("object_call", () => String(objProxy()));
console.log("object_trap_reached=" + applyReached);
console.log("object_typeof=" + typeof objProxy);

// a generator function is callable but not constructable
function* gen() { yield 1; }
const genProxy: any = new Proxy(gen, { construct() { return {}; } });
console.log("gen_call=" + genProxy().next().value);
attempt("gen_new", () => String(typeof new genProxy()));

// an async function likewise
const asyncProxy: any = new Proxy(async function a() { return 1; }, { construct() { return {}; } });
console.log("async_typeof=" + typeof asyncProxy);
attempt("async_new", () => String(typeof new asyncProxy()));

// the construct trap MUST return an object
function Base(this: any) { (this as any).b = 1; }
const returnsNumber: any = new Proxy(Base, { construct() { return 5 as any; } });
attempt("construct_number", () => String(typeof new returnsNumber()));
const returnsUndefined: any = new Proxy(Base, { construct() { return undefined as any; } });
attempt("construct_undefined", () => String(typeof new returnsUndefined()));
const returnsNull: any = new Proxy(Base, { construct() { return null as any; } });
attempt("construct_null", () => String(typeof new returnsNull()));
const returnsFn: any = new Proxy(Base, { construct() { return function inner() { return 1; }; } });
console.log("construct_function=" + typeof new returnsFn());
const returnsArray: any = new Proxy(Base, { construct() { return [1, 2]; } });
console.log("construct_array=" + Array.isArray(new returnsArray()));

// the apply trap has no such rule
console.log("apply_number=" + new Proxy(Base, { apply() { return 5; } })());
console.log("apply_undefined=" + String(new Proxy(Base, { apply() { return undefined; } })()));

// what the traps receive: thisArg and a real Array of arguments, and the
// newTarget of a plain `new` is the proxy itself
const info: string[] = [];
const spy: any = new Proxy(Base, {
  apply(t, thisArg, args) {
    info.push("apply:this=" + (thisArg === null ? "null" : typeof thisArg) + ",args=" + Array.isArray(args) + ":" + args.length + ":" + args.join("/"));
    return "A";
  },
  construct(t, args, nt) {
    info.push("construct:args=" + Array.isArray(args) + ":" + args.join("/") + ",nt_is_proxy=" + (nt === spy));
    return { tag: "C" };
  },
});
console.log("apply_ret=" + spy(1, 2));
console.log("call_ret=" + spy.call({ x: 1 }, 3));
console.log("applied_ret=" + spy.apply(null, [4, 5]));
console.log("reflect_apply=" + Reflect.apply(spy, undefined, [6]));
console.log("construct_ret=" + new spy(7, 8).tag);
console.log("info=" + info.join("|"));

// Reflect.construct hands a DIFFERENT newTarget straight to the trap
let sawNt = "none";
const ntSpy: any = new Proxy(Base, {
  construct(t, args, nt) { sawNt = nt === Other ? "Other" : nt === ntSpy ? "self" : "?"; return Reflect.construct(t as any, args, nt); },
});
function Other(this: any) { /* noop */ }
(Other as any).prototype.marker = "OTHER";
const viaNt: any = Reflect.construct(ntSpy, [], Other);
console.log("newTarget=" + sawNt + ",marker=" + viaNt.marker);

// a bound function keeps its target's constructability through a proxy
const boundBase: any = (Base as any).bind(null);
const boundProxy: any = new Proxy(boundBase, {});
console.log("bound_new=" + new boundProxy().b);
console.log("bound_typeof=" + typeof boundProxy + ",name=" + boundProxy.name);
console.log("proxy_of_proxy_call=" + new Proxy(new Proxy(Base, { apply() { return "INNER"; } }), {})());
