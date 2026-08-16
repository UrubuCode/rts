// Cross-runtime: the EDGES of the global symbol registry — key coercion in
// Symbol.for, what Symbol.keyFor answers for a symbol that is not registered,
// and the fact that the registry key is independent of the description.

// --- the key is coerced with ToString, so distinct arguments can collide ---
console.log("num_key=" + (Symbol.for(1 as any) === Symbol.for("1")));
console.log("undef_key=" + (Symbol.for(undefined as any) === Symbol.for("undefined")));
console.log("null_key=" + (Symbol.for(null as any) === Symbol.for("null")));
console.log("bool_key=" + (Symbol.for(true as any) === Symbol.for("true")));
console.log("noarg_key=" + ((Symbol.for as any)() === Symbol.for("undefined")));
console.log("empty_key=" + (Symbol.for("") === Symbol.for("")));
console.log("empty_keyfor=" + JSON.stringify(Symbol.keyFor(Symbol.for(""))));

// --- an object argument goes through toString ---
const keyish = { toString() { return "claude-obj-key"; } };
console.log("obj_key=" + (Symbol.for(keyish as any) === Symbol.for("claude-obj-key")));
console.log("obj_keyfor=" + Symbol.keyFor(Symbol.for(keyish as any)));

// (Symbol.for with a SYMBOL argument is deliberately not asserted here: Node
// throws on the ToString step and the other runtime does not, so there is no
// shared answer.)

// --- the registry key IS the description for a registered symbol ---
const reg = Symbol.for("claude-reg");
console.log("reg_description=" + reg.description);
console.log("reg_keyfor=" + Symbol.keyFor(reg));
console.log("reg_tostring=" + reg.toString());

// --- but a plain Symbol with the same description is a different symbol ---
const plain = Symbol("claude-reg");
console.log("plain_vs_reg=" + (plain === reg));
console.log("plain_same_description=" + (plain.description === reg.description));
console.log("plain_keyfor=" + String(Symbol.keyFor(plain)));

// --- keyFor over symbols that were never registered ---
console.log("keyfor_unique=" + String(Symbol.keyFor(Symbol("nope"))));
console.log("keyfor_bare=" + String(Symbol.keyFor(Symbol())));
console.log("keyfor_iterator=" + String(Symbol.keyFor(Symbol.iterator)));
console.log("keyfor_asyncIterator=" + String(Symbol.keyFor(Symbol.asyncIterator)));
console.log("keyfor_toPrimitive=" + String(Symbol.keyFor(Symbol.toPrimitive)));

// --- keyFor refuses a non-symbol ---
function bad(label: string, fn: () => any): void {
  try { fn(); console.log(label + "=no_throw"); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
bad("keyfor_string", () => (Symbol.keyFor as any)("x"));
bad("keyfor_number", () => (Symbol.keyFor as any)(1));
bad("keyfor_object", () => (Symbol.keyFor as any)({}));
bad("keyfor_null", () => (Symbol.keyFor as any)(null));
bad("keyfor_noarg", () => (Symbol.keyFor as any)());
bad("keyfor_wrapper", () => (Symbol.keyFor as any)(Object(Symbol("w"))));

// --- the registry is global and survives repeated lookups ---
const r1 = Symbol.for("claude-stable");
const r2 = Symbol.for("claude-stable");
const r3 = Symbol.for("claude-stable");
console.log("stable=" + (r1 === r2) + ":" + (r2 === r3));
console.log("stable_typeof=" + typeof r1);

// --- a registered symbol works as an ordinary property key ---
const holder: any = {};
holder[r1] = 42;
console.log("as_key=" + holder[Symbol.for("claude-stable")]);
console.log("own_symbols=" + Object.getOwnPropertySymbols(holder).length);
console.log("own_symbol_keyfor=" + Symbol.keyFor(Object.getOwnPropertySymbols(holder)[0]));

// --- shape of the statics themselves ---
console.log("for_length=" + Symbol.for.length + ":" + Symbol.for.name);
console.log("keyFor_length=" + Symbol.keyFor.length + ":" + Symbol.keyFor.name);
const fd: any = Object.getOwnPropertyDescriptor(Symbol, "for");
console.log("for_flags=" + fd.writable + ":" + fd.enumerable + ":" + fd.configurable);
