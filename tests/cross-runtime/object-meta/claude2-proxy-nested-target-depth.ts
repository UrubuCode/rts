// Pins a proxy whose TARGET is another proxy: every operation walks the chain
// outermost-first, each level's trap sees the next proxy as its target, and the
// structural questions (typeof, Array.isArray, instanceof) pierce all levels.

const log: string[] = [];

function layer(target: any, tag: string): any {
  return new Proxy(target, {
    get(t, k, r) { log.push("get" + tag + ":" + String(k)); return Reflect.get(t, k, r); },
    set(t, k, v, r) { log.push("set" + tag + ":" + String(k)); return Reflect.set(t, k, v, r); },
    has(t, k) { log.push("has" + tag + ":" + String(k)); return Reflect.has(t, k); },
    ownKeys(t) { log.push("keys" + tag); return Reflect.ownKeys(t); },
    getOwnPropertyDescriptor(t, k) { log.push("gopd" + tag + ":" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
    deleteProperty(t, k) { log.push("del" + tag + ":" + String(k)); return Reflect.deleteProperty(t, k); },
    getPrototypeOf(t) { log.push("proto" + tag); return Reflect.getPrototypeOf(t); },
  });
}

const root: any = { a: 1, b: 2 };
const one: any = layer(root, "1");
const two: any = layer(one, "2");
const three: any = layer(two, "3");

function run(label: string, fn: () => void): void {
  log.length = 0;
  fn();
  console.log(label + "=" + log.join(","));
}

run("read", () => { void three.a; });
run("write", () => { Reflect.set(three, "c", 3); });
run("has", () => { void ("a" in three); });
run("delete", () => { Reflect.deleteProperty(three, "c"); });
run("ownKeys", () => { Reflect.ownKeys(three); });
run("keys", () => { Object.keys(three); });
run("proto", () => { Object.getPrototypeOf(three); });

console.log("value=" + three.a + ":" + three.b);
console.log("root_after=" + Object.keys(root).join("|"));

// each level's trap receives the NEXT proxy down as its target, never the root
const seen: string[] = [];
const inner: any = new Proxy(root, { get(t, k, r) { seen.push("inner_target_is_root:" + (t === root)); return Reflect.get(t, k, r); } });
const outer: any = new Proxy(inner, { get(t, k, r) { seen.push("outer_target_is_inner:" + (t === inner)); return Reflect.get(t, k, r); } });
void outer.a;
console.log("targets=" + seen.join(","));

// the receiver stays the OUTERMOST proxy all the way down
const recv: string[] = [];
const rInner: any = new Proxy({ z: 1 }, { get(t, k, r) { recv.push("inner:" + (r === rOuter)); return Reflect.get(t, k, r); } });
const rOuter: any = new Proxy(rInner, { get(t, k, r) { recv.push("outer:" + (r === rOuter)); return Reflect.get(t, k, r); } });
void rOuter.z;
console.log("receiver=" + recv.join(","));

// structural questions pierce every level
const arr: any = [1, 2, 3];
const a1: any = new Proxy(arr, {});
const a2: any = new Proxy(a1, {});
const a3: any = new Proxy(a2, {});
console.log("isArray=" + Array.isArray(a1) + ":" + Array.isArray(a2) + ":" + Array.isArray(a3));
console.log("arr_tag=" + Object.prototype.toString.call(a3));
console.log("arr_len=" + a3.length);
console.log("arr_json=" + JSON.stringify(a3));

function fn(x: number): number { return x + 1; }
const f1: any = new Proxy(fn, {});
const f2: any = new Proxy(f1, {});
const f3: any = new Proxy(f2, {});
console.log("typeof=" + typeof f1 + ":" + typeof f2 + ":" + typeof f3);
console.log("call=" + f3(1));
console.log("fn_name=" + f3.name + ",len=" + f3.length);

class K { }
const k1: any = new Proxy(K, {});
const k2: any = new Proxy(k1, {});
console.log("nested_new=" + (new k2() instanceof K));

// a trap at ONE level intercepts even though the levels around it forward
const filtered: any = new Proxy(new Proxy({ x: 1 }, { get() { return "MIDDLE"; } }), {});
console.log("middle_wins=" + filtered.x);

// revoking the MIDDLE proxy poisons everything outside it
const mid = Proxy.revocable({ q: 1 }, {});
const outside: any = new Proxy(mid.proxy, {});
console.log("before_revoke=" + outside.q);
mid.revoke();
try {
  console.log("after_revoke=" + outside.q);
} catch (e: any) {
  console.log("after_revoke=throw:" + e.constructor.name);
}
