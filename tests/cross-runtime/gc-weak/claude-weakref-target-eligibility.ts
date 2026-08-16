// Cross-runtime: which values a WeakRef may TARGET, and what deref answers while
// a strong reference is still held. Nothing here asserts that anything was ever
// collected — GC timing is not observable portably, so only the guaranteed
// liveness and the API surface are pinned.

// --- deref returns the very same object while a strong reference exists ---
const target: any = { id: 7 };
const ref = new WeakRef(target);
console.log("deref_identity=" + (ref.deref() === target));
console.log("deref_twice_same=" + (ref.deref() === ref.deref()));
console.log("deref_typeof=" + typeof ref.deref());
console.log("deref_value=" + (ref.deref() as any).id);
target.id = 8;
console.log("deref_sees_mutation=" + (ref.deref() as any).id);

// --- two WeakRefs to one target are distinct objects with the same target ---
const ref2 = new WeakRef(target);
console.log("refs_distinct=" + (ref === ref2));
console.log("targets_same=" + (ref.deref() === ref2.deref()));

// --- functions, arrays and class instances are all valid targets ---
function fnTarget() { return 1; }
const arrTarget: any = [1, 2];
class Inst { v = 3; }
const instTarget = new Inst();
console.log("function_target=" + (new WeakRef(fnTarget).deref() === fnTarget));
console.log("array_target=" + (new WeakRef(arrTarget).deref() === arrTarget));
console.log("class_target=" + ((new WeakRef(instTarget).deref() as any).v));
console.log("proxy_target=" + (typeof new WeakRef(new Proxy({}, {})).deref()));

// --- an UNREGISTERED symbol is a valid target (ES2023) ---
const symTarget = Symbol("weak");
const symRef = new WeakRef(symTarget as any);
console.log("symbol_target=" + ((symRef.deref() as any) === symTarget));
console.log("symbol_deref_typeof=" + typeof symRef.deref());
console.log("wellknown_symbol_target=" + ((new WeakRef(Symbol.iterator as any).deref() as any) === Symbol.iterator));

// --- but a REGISTERED symbol is refused ---
function bad(label: string, fn: () => any): void {
  try { const v = fn(); console.log(label + "=no_throw:" + typeof v); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
bad("registered_symbol", () => new WeakRef(Symbol.for("claude-weakref-key") as any));

// --- every other primitive is refused ---
bad("number", () => new WeakRef(1 as any));
bad("string", () => new WeakRef("s" as any));
bad("boolean", () => new WeakRef(true as any));
bad("null", () => new WeakRef(null as any));
bad("undefined", () => new WeakRef(undefined as any));
bad("bigint", () => new WeakRef(10n as any));
bad("no_argument", () => new (WeakRef as any)());

// --- WeakRef demands `new` ---
bad("call_without_new", () => (WeakRef as any)({}));

// --- deref has a brand check ---
bad("deref_on_plain", () => WeakRef.prototype.deref.call({} as any));
bad("deref_on_object_target", () => WeakRef.prototype.deref.call(target as any));
bad("deref_on_prototype", () => (WeakRef.prototype as any).deref());
console.log("deref_via_call=" + (WeakRef.prototype.deref.call(ref) === target));

// --- API shape ---
console.log("ctor_length=" + WeakRef.length + ":" + WeakRef.name);
console.log("deref_length=" + WeakRef.prototype.deref.length + ":" + WeakRef.prototype.deref.name);
console.log("tag=" + Object.prototype.toString.call(ref));
const td: any = Object.getOwnPropertyDescriptor(WeakRef.prototype, Symbol.toStringTag);
console.log("tag_desc=" + td.value + ":" + td.writable + ":" + td.enumerable + ":" + td.configurable);
console.log("instance_own_keys=" + Reflect.ownKeys(ref).length);
console.log("proto_ctor=" + (WeakRef.prototype.constructor === WeakRef));
console.log("instanceof=" + (ref instanceof WeakRef));

// --- a subclass works and keeps the internal slot ---
class TrackedRef extends WeakRef<any> {
  label = "tracked";
}
const tracked = new TrackedRef(target);
console.log("subclass_deref=" + (tracked.deref() === target));
console.log("subclass_label=" + tracked.label);
console.log("subclass_tag=" + Object.prototype.toString.call(tracked));
console.log("subclass_instanceof=" + (tracked instanceof WeakRef) + ":" + (tracked instanceof TrackedRef));

// --- a WeakRef is itself an ordinary object: usable as a Map key and a
//     WeakMap key ---
const holder = new WeakMap<any, string>();
holder.set(ref, "held");
console.log("as_weakmap_key=" + holder.get(ref));
console.log("as_set_member=" + new Set([ref, ref]).size);
