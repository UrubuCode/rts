// Pins the receiver argument of Reflect.get: it is the `this` an accessor runs
// with and nothing else — a data property ignores it entirely, an inherited
// getter reads the receiver's own slots, and a proxy is handed it verbatim.

class Reader {
  static tagOf(v: any): string {
    if (v === null) return "null";
    if (typeof v !== "object" && typeof v !== "function") return typeof v + ":" + String(v);
    return String((v as any).tag);
  }
}

const proto: any = {
  get who() { return "who:" + Reader.tagOf(this); },
  get slot() { return "slot:" + String((this as any).value); },
  plain: "DATA",
};
const owner: any = Object.create(proto);
owner.tag = "owner";
owner.value = 1;
const substitute: any = { tag: "substitute", value: 2 };

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

console.log("default_receiver=" + Reflect.get(owner, "who"));
console.log("explicit_owner=" + Reflect.get(owner, "who", owner));
console.log("substituted=" + Reflect.get(owner, "who", substitute));
console.log("slot_owner=" + Reflect.get(owner, "slot"));
console.log("slot_substituted=" + Reflect.get(owner, "slot", substitute));
console.log("getter_on_proto_direct=" + Reflect.get(proto, "who", substitute));

// a data property ignores the receiver completely
console.log("data_ignores=" + Reflect.get(owner, "plain", substitute));
console.log("data_own_ignores=" + Reflect.get(owner, "tag", substitute));
console.log("missing=" + String(Reflect.get(owner, "nope", substitute)));

// a primitive receiver reaches the getter unchanged, because a class body is
// always strict — no boxing, whatever mode the surrounding file has
class StrictHost {
  static describe(o: any, recv: any): string {
    return Reflect.get(o, "typeofThis", recv);
  }
}
// the GETTER must live in a class body too: a primitive `this` is boxed in
// sloppy mode and left alone in strict, and a class body is always strict
class TypeProbe {
  get typeofThis(): string { return typeof this + ":" + String(this); }
}
const typeProbe: any = new TypeProbe();
console.log("recv_number=" + StrictHost.describe(typeProbe, 5));
console.log("recv_string=" + StrictHost.describe(typeProbe, "s"));
console.log("recv_boolean=" + StrictHost.describe(typeProbe, true));
console.log("recv_null=" + StrictHost.describe(typeProbe, null));
console.log("recv_undefined=" + StrictHost.describe(typeProbe, undefined));
console.log("recv_symbol=" + StrictHost.describe(typeProbe, Symbol("r")).slice(0, 12));

// a proxy receives the receiver verbatim, including a receiver that is not
// related to the target at all
const seen: string[] = [];
const proxied: any = new Proxy({ k: "T" }, {
  // identity, not a property read: reading .tag off the receiver would re-enter
  // this very trap when the receiver IS the proxy
  get(t, k, r) {
    const which = r === proxied ? "proxy" : r === substitute ? "substitute" : typeof r + ":" + String(r);
    seen.push(String(k) + ":" + which);
    return Reflect.get(t, k, r);
  },
});
Reflect.get(proxied, "k");
Reflect.get(proxied, "k", substitute);
Reflect.get(proxied, "k", 7 as any);
console.log("proxy_receivers=" + seen.join("|"));

// forwarding the receiver through Reflect.get inside a trap keeps an inherited
// getter bound to the ORIGINAL receiver
const base: any = { get computed() { return "computed:" + String((this as any).value); } };
const targetWithProto: any = Object.create(base);
targetWithProto.value = "target";
const forwarding: any = new Proxy(targetWithProto, { get(t, k, r) { return Reflect.get(t, k, r); } });
console.log("proxy_getter_default=" + forwarding.computed);
console.log("proxy_getter_substituted=" + Reflect.get(forwarding, "computed", { value: "sub" }));

// dropping the receiver inside the trap changes which object the getter reads
const dropping: any = new Proxy(targetWithProto, { get(t, k) { return Reflect.get(t, k); } });
console.log("dropped_receiver=" + Reflect.get(dropping, "computed", { value: "sub" }));

// Reflect.get on an array and on a boxed string
console.log("array_length=" + Reflect.get([1, 2, 3], "length"));
console.log("array_index=" + Reflect.get([1, 2, 3], "1"));
console.log("boxed_index=" + Reflect.get(new String("hi"), "0"));
attempt("primitive_target", () => String(Reflect.get("hi" as any, "0")));

// a setter counterpart: Reflect.set with a receiver runs the setter with it
const setterLog: string[] = [];
const setterProto: any = { set record(v: any) { setterLog.push(Reader.tagOf(this) + "<-" + v); } };
const setterOwner: any = Object.create(setterProto);
setterOwner.tag = "setterOwner";
console.log("set_default=" + Reflect.set(setterOwner, "record", 1));
console.log("set_substituted=" + Reflect.set(setterOwner, "record", 2, substitute));
console.log("setter_log=" + setterLog.join("|"));
console.log("no_own_created=" + Object.getOwnPropertyNames(setterOwner).sort().join("|"));
