// Pins a proxy used as a PROTOTYPE: the traps fire for a lookup that misses on
// the child, the receiver they are handed is the CHILD and not the proxy, an
// own property on the child stops the walk, and `in` reaches the has trap.

const log: string[] = [];

const target: any = { inherited: "T" };
const proxy: any = new Proxy(target, {
  get(t, k, r) { log.push("get:" + String(k) + ":recv=" + tagOf(r)); return Reflect.get(t, k, r); },
  set(t, k, v, r) { log.push("set:" + String(k) + ":recv=" + tagOf(r)); return Reflect.set(t, k, v, r); },
  has(t, k) { log.push("has:" + String(k)); return Reflect.has(t, k); },
  getOwnPropertyDescriptor(t, k) { log.push("gopd:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
  ownKeys(t) { log.push("ownKeys"); return Reflect.ownKeys(t); },
  getPrototypeOf(t) { log.push("getProto"); return Reflect.getPrototypeOf(t); },
});

const child: any = Object.create(proxy);
child.own = "C";

function tagOf(v: any): string {
  if (v === child) return "child";
  if (v === proxy) return "proxy";
  if (v === target) return "target";
  return "other";
}

function run(label: string, fn: () => void): void {
  log.length = 0;
  fn();
  console.log(label + "=" + log.join(","));
}

run("miss_on_child", () => { void child.inherited; });
run("hit_on_child", () => { void child.own; });
run("absent_everywhere", () => { void child.nowhere; });
run("in_operator", () => { void ("inherited" in child); });
run("in_own", () => { void ("own" in child); });
run("hasOwn_child", () => { Object.hasOwn(child, "inherited"); });
run("keys_child", () => { Object.keys(child); });
run("forin_child", () => { for (const _k in child) { void _k; } });

console.log("value=" + child.inherited + ":" + child.own);
console.log("forin_names=" + (() => { const a: string[] = []; for (const k in child) a.push(k); return a.join("|"); })());

// a write that misses reaches the proxy's set trap with the child as receiver;
// forwarding it through Reflect.set creates the property on the CHILD
log.length = 0;
console.log("write_miss=" + Reflect.set(child, "fresh", "F"));
console.log("write_log=" + log.join(","));
console.log("child_has_fresh=" + Object.hasOwn(child, "fresh") + ",target=" + Object.hasOwn(target, "fresh"));

// an accessor behind the proxy runs with `this` bound to the child
const accTarget: any = {
  get doubled() { return "this_is_" + accTag(this) + ":" + (this as any).own; },
  set doubled(v: any) { (this as any).stored = "from_" + accTag(this) + ":" + v; },
};
function accTag(v: any): string {
  if (v === accChild) return "accChild";
  if (v === accProxy) return "accProxy";
  if (v === accTarget) return "accTarget";
  return "other";
}
const accProxy: any = new Proxy(accTarget, {
  get(t, k, r) { return Reflect.get(t, k, r); },
  set(t, k, v, r) { return Reflect.set(t, k, v, r); },
});
const accChild: any = Object.create(accProxy);
accChild.own = "C2";
console.log("acc_get=" + accChild.doubled);
console.log("acc_set=" + Reflect.set(accChild, "doubled", "V"));
console.log("acc_stored=" + accChild.stored + ",own_on_child=" + Object.hasOwn(accChild, "stored"));
console.log("acc_target_clean=" + Object.getOwnPropertyNames(accTarget).sort().join("|"));

// the DEFAULT receiver of a direct read is the proxy itself, and Reflect.get
// can substitute any other object
const recvLog: string[] = [];
const other: any = { tag: "other" };
const direct: any = new Proxy({ v: 1 }, {
  get(t, k, r) { recvLog.push(String(k) + ":" + (r === direct ? "proxy" : r === other ? "other" : "?")); return Reflect.get(t, k, r); },
});
void direct.v;
Reflect.get(direct, "v", other);
console.log("receivers=" + recvLog.join(","));

// a proxy deep in the chain still sees the original child as receiver
const deepTarget: any = { deep: "D" };
let deepRecv = "none";
const deepProxy: any = new Proxy(deepTarget, { get(t, k, r) { deepRecv = (r === grandchild ? "grandchild" : "other"); return Reflect.get(t, k, r); } });
const middle: any = Object.create(deepProxy);
const grandchild: any = Object.create(middle);
console.log("deep_value=" + grandchild.deep);
console.log("deep_receiver=" + deepRecv);
