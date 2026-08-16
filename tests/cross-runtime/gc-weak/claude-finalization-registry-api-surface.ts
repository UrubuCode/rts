// Cross-runtime: the FinalizationRegistry API surface — what register/unregister
// accept and return, and the guarantee that the cleanup callback is NEVER
// invoked synchronously. Nothing here forces or awaits a collection.

let callbackRuns = 0;
const fr = new FinalizationRegistry((held: any) => { callbackRuns++; void held; });

const a: any = { id: "a" };
const b: any = { id: "b" };
const token: any = { id: "token" };

// --- register returns undefined, and never calls back on the spot ---
console.log("register_returns=" + String(fr.register(a, "heldA")));
console.log("register_with_token=" + String(fr.register(b, "heldB", token)));
console.log("callback_runs=" + callbackRuns);

// --- the same target may be registered more than once ---
console.log("register_again=" + String(fr.register(a, "heldA2")));
console.log("register_same_token_twice=" + String(fr.register(a, "heldA3", token)));

// --- unregister returns a boolean saying whether anything was removed ---
console.log("unregister_known=" + fr.unregister(token));
console.log("unregister_again=" + fr.unregister(token));
console.log("unregister_unknown=" + fr.unregister({} as any));

// --- still nothing ran ---
console.log("callback_runs_after=" + callbackRuns);

function bad(label: string, fn: () => any): void {
  try { const v = fn(); console.log(label + "=no_throw:" + String(v)); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}

// --- a target that cannot be held weakly is refused ---
bad("register_number", () => fr.register(1 as any, "h"));
bad("register_string", () => fr.register("s" as any, "h"));
bad("register_null", () => fr.register(null as any, "h"));
bad("register_undefined", () => fr.register(undefined as any, "h"));
bad("register_bigint", () => fr.register(10n as any, "h"));
bad("register_registered_symbol", () => fr.register(Symbol.for("claude-fr-key") as any, "h"));
console.log("register_unique_symbol=" + String(fr.register(Symbol("uniq") as any, "h")));

// --- a target cannot be its own held value ---
bad("target_is_held_value", () => fr.register(a, a));
console.log("held_value_may_be_anything=" + String(fr.register(a, undefined)) + ":" + String(fr.register(a, 42)));
console.log("held_value_other_object=" + String(fr.register(a, b)));

// --- the unregister token must be holdable ---
bad("token_number", () => fr.register(a, "h", 1 as any));
bad("token_string", () => fr.register(a, "h", "tok" as any));
console.log("token_undefined_ok=" + String(fr.register(a, "h", undefined)));
bad("unregister_primitive", () => fr.unregister("tok" as any));
bad("unregister_number", () => fr.unregister(1 as any));
bad("unregister_undefined", () => (fr.unregister as any)(undefined));
console.log("unregister_unique_symbol=" + fr.unregister(Symbol("t") as any));

// --- the constructor demands a callable and `new` ---
bad("ctor_no_callback", () => new (FinalizationRegistry as any)());
bad("ctor_number_callback", () => new (FinalizationRegistry as any)(42));
bad("ctor_object_callback", () => new (FinalizationRegistry as any)({}));
bad("ctor_without_new", () => (FinalizationRegistry as any)(() => {}));
console.log("ctor_arrow_ok=" + (typeof new FinalizationRegistry(() => {})));

// --- brand checks on the methods ---
bad("register_on_plain", () => FinalizationRegistry.prototype.register.call({} as any, a, "h"));
bad("unregister_on_plain", () => FinalizationRegistry.prototype.unregister.call({} as any, token));
bad("register_on_prototype", () => (FinalizationRegistry.prototype as any).register(a, "h"));

// --- API shape ---
console.log("ctor_length=" + FinalizationRegistry.length + ":" + FinalizationRegistry.name);
console.log("register_length=" + fr.register.length + ":" + fr.register.name);
console.log("unregister_length=" + fr.unregister.length + ":" + fr.unregister.name);
console.log("tag=" + Object.prototype.toString.call(fr));
const td: any = Object.getOwnPropertyDescriptor(FinalizationRegistry.prototype, Symbol.toStringTag);
console.log("tag_desc=" + td.value + ":" + td.writable + ":" + td.enumerable + ":" + td.configurable);
console.log("own_keys=" + Reflect.ownKeys(fr).length);
console.log("proto_ctor=" + (FinalizationRegistry.prototype.constructor === FinalizationRegistry));
console.log("has_cleanupSome=" + ("cleanupSome" in FinalizationRegistry.prototype));

// --- a subclass keeps the slot ---
class Tracked extends FinalizationRegistry<any> {
  label = "tracked";
}
const tr = new Tracked(() => { /* never called synchronously */ });
console.log("subclass_register=" + String(tr.register(a, "h")));
console.log("subclass_unregister=" + tr.unregister({} as any));
console.log("subclass_label=" + tr.label + ":" + (tr instanceof FinalizationRegistry));

// --- after all of that, the callback has still never run ---
console.log("final_callback_runs=" + callbackRuns);
