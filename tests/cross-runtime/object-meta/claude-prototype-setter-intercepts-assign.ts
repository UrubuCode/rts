// Pins that an inherited SETTER swallows an assignment: no own property is
// created on the child, `this` inside the setter is the child, and only
// defineProperty can shadow the accessor. 341_getter_inheritance reads through
// the chain but never writes through it.

const store: string[] = [];

const proto: any = {};
Object.defineProperty(proto, "p", {
  get(this: any) { return "get<" + (this.tag || "?") + ">"; },
  set(this: any, v: string) { store.push(v + "@" + (this.tag || "?")); this._backing = v; },
  enumerable: true,
  configurable: true,
});
Object.defineProperty(proto, "readonly", {
  get() { return "RO"; },
  enumerable: true,
  configurable: true,
});

const child: any = Object.create(proto);
child.tag = "CHILD";

child.p = "v1";
console.log("store=" + store.join(","));
console.log("child_own_p=" + Object.prototype.hasOwnProperty.call(child, "p"));
console.log("child_own_backing=" + Object.prototype.hasOwnProperty.call(child, "_backing") + ":" + child._backing);
console.log("proto_own_backing=" + Object.prototype.hasOwnProperty.call(proto, "_backing"));
console.log("read=" + child.p);
console.log("keys=" + Object.keys(child).join("|"));

// a second child gets its own backing, the accessor is shared
const other: any = Object.create(proto);
other.tag = "OTHER";
other.p = "v2";
console.log("store2=" + store.join(","));
console.log("other_backing=" + other._backing + ",child_backing=" + child._backing);

// a getter-only inherited accessor refuses the write, and the refusal is read
// as a boolean rather than as a throw: whether an assignment throws is a
// property of the CALLER's mode, which differs between a script and a module
function write(target: any, key: string, value: unknown): string {
  return String(Reflect.set(target, key, value));
}
console.log("write_readonly=" + write(child, "readonly", "x"));
console.log("readonly_own=" + Object.prototype.hasOwnProperty.call(child, "readonly"));
console.log("readonly_read=" + child.readonly);

// defineProperty on the CHILD shadows the inherited accessor with a data slot
Object.defineProperty(child, "p", { value: "OWN", writable: true, enumerable: true, configurable: true });
console.log("shadow_read=" + child.p);
console.log("shadow_own=" + Object.prototype.hasOwnProperty.call(child, "p"));
console.log("write_after_shadow=" + write(child, "p", "OWN2") + ":" + child.p);
console.log("store_unchanged=" + store.join(","));
console.log("other_still_accessor=" + other.p);

// deleting the shadow restores the inherited accessor
delete child.p;
console.log("after_delete=" + child.p);
console.log("write_after_delete=" + write(child, "p", "v3") + ",store=" + store.join(","));

// a setter inherited through TWO levels still binds `this` to the receiver
const mid: any = Object.create(proto);
const deep: any = Object.create(mid);
deep.tag = "DEEP";
deep.p = "v4";
console.log("deep_store=" + store[store.length - 1]);
console.log("deep_own_backing=" + Object.prototype.hasOwnProperty.call(deep, "_backing"));
console.log("mid_own_backing=" + Object.prototype.hasOwnProperty.call(mid, "_backing"));

// a class ACCESSOR lives on the prototype and behaves the same way
class Box {
  get size(): number { return (this as any)._s || 0; }
  set size(v: number) { (this as any)._s = v * 2; }
}
const b: any = new Box();
b.size = 5;
console.log("class_size=" + b.size);
console.log("class_own=" + Object.getOwnPropertyNames(b).join("|"));
const bd = Object.getOwnPropertyDescriptor(Box.prototype, "size") as any;
console.log("class_desc=e=" + bd.enumerable + ",c=" + bd.configurable + ",get=" + typeof bd.get);

// Object.assign onto a target whose PROTOTYPE has the setter routes through it
const dest: any = Object.create(proto);
dest.tag = "DEST";
Object.assign(dest, { p: "v5", z: 1 });
console.log("assign_store=" + store[store.length - 1]);
console.log("assign_own=" + Object.getOwnPropertyNames(dest).join("|"));

// spread never triggers a setter: it defines own properties
const spreadDest: any = { ...({ p: "v6" }) };
console.log("spread_store_tail=" + store[store.length - 1]);
console.log("spread_own=" + Object.getOwnPropertyNames(spreadDest).join("|") + ":" + spreadDest.p);
