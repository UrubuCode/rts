// Cross-runtime: what unregister() actually returns. The boolean is the only
// window onto a registry's bookkeeping: one token may cover several cells and
// clears them all at once, a token is scoped to the registry it was used on,
// and a token is compared by identity, never by the target it accompanied.
// Nothing here waits for, forces, or asserts a collection.

let calls = 0;
const fr = new FinalizationRegistry((held: any) => { calls++; void held; });

const a: any = { id: "a" };
const b: any = { id: "b" };
const c: any = { id: "c" };
const tok1: any = { id: "tok1" };
const tok2: any = { id: "tok2" };

// --- one token covering three cells clears in a single call ---
fr.register(a, "a1", tok1);
fr.register(b, "b1", tok1);
fr.register(c, "c1", tok1);
console.log("multi_first=" + fr.unregister(tok1));
console.log("multi_again=" + fr.unregister(tok1));
console.log("multi_third=" + fr.unregister(tok1));

// --- an unused token is simply false ---
console.log("never_used=" + fr.unregister(tok2));
console.log("fresh_object=" + fr.unregister({ id: "fresh" }));

// --- registering without a token means it can never be unregistered ---
fr.register(a, "no_token");
console.log("no_token_by_target=" + fr.unregister(a));
console.log("no_token_by_other=" + fr.unregister(tok2));

// --- the target itself is a legal token, and then unregistering by it works ---
fr.register(b, "self_token", b);
console.log("target_as_token=" + fr.unregister(b));
console.log("target_as_token_again=" + fr.unregister(b));

// --- but the target of a cell registered under ANOTHER token is not a key ---
fr.register(c, "under_tok2", tok2);
console.log("wrong_key_is_target=" + fr.unregister(c));
console.log("right_key_is_token=" + fr.unregister(tok2));

// --- identity, not equality: a structurally identical token misses ---
const shaped1: any = { same: 1 };
const shaped2: any = { same: 1 };
fr.register(a, "shaped", shaped1);
console.log("lookalike_token=" + fr.unregister(shaped2));
console.log("real_token=" + fr.unregister(shaped1));

// --- a registered token stays usable after the cell it covered is gone ---
fr.register(a, "again", tok1);
console.log("reused_token=" + fr.unregister(tok1));
console.log("reused_token_again=" + fr.unregister(tok1));

// --- two registries keep separate books over the same token ---
const other = new FinalizationRegistry(() => { calls++; });
fr.register(a, "in_fr", tok1);
other.register(a, "in_other", tok1);
console.log("cross_first=" + other.unregister(tok1));
console.log("cross_other_empty=" + other.unregister(tok1));
console.log("cross_original_intact=" + fr.unregister(tok1));

// --- the same target may hold many cells with different tokens ---
fr.register(a, "h1", tok1);
fr.register(a, "h2", tok2);
fr.register(a, "h3");
console.log("many_cells_tok1=" + fr.unregister(tok1));
console.log("many_cells_tok2=" + fr.unregister(tok2));
console.log("many_cells_tok1_again=" + fr.unregister(tok1));

// --- a symbol works as a token, with the same unregistrable/registered split ---
const symTok: any = Symbol("token");
fr.register(a, "sym", symTok);
console.log("symbol_token=" + fr.unregister(symTok));
console.log("symbol_token_again=" + fr.unregister(symTok));
try { fr.register(a, "reg", Symbol.for("claude2-fr-token") as any); console.log("registered_symbol_token=no_throw"); }
catch (e: any) { console.log("registered_symbol_token=" + e.constructor.name); }

// --- a Proxy token is its own identity, and no trap fires ---
const trapLog: string[] = [];
const rawTok: any = { id: "raw" };
const proxyTok: any = new Proxy(rawTok, {
  get(t: any, k: any, r: any) { trapLog.push("get:" + String(k)); return Reflect.get(t, k, r); },
  has(t: any, k: any) { trapLog.push("has"); return Reflect.has(t, k); },
});
fr.register(a, "proxy_tok", proxyTok);
console.log("proxy_token_raw_misses=" + fr.unregister(rawTok));
console.log("proxy_token_hits=" + fr.unregister(proxyTok));
console.log("proxy_traps=" + JSON.stringify(trapLog.join("|")));

// --- the return type is a genuine boolean ---
console.log("return_type=" + typeof fr.unregister(tok1));
console.log("strict_false=" + (fr.unregister(tok1) === false));
fr.register(a, "typed", tok1);
console.log("strict_true=" + (fr.unregister(tok1) === true));

// --- everything above ran without the callback ever being invoked ---
console.log("callback_calls=" + calls);

// --- register answers undefined every time, whatever it did ---
console.log("register_returns=" + String(fr.register(a, "x")) + ":" + String(fr.register(a, "y", tok1)) + ":" + String(fr.register(a, "z", undefined)));

// --- a held value may be anything except the target itself ---
function attempt(label: string, fn: () => any): void {
  try { console.log(label + "=ok:" + String(fn())); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
attempt("held_is_target", () => fr.register(c, c));
attempt("held_is_token", () => fr.register(c, tok1, tok1));
attempt("held_is_registry", () => fr.register(c, fr));
attempt("held_is_symbol", () => fr.register(c, Symbol("h")));
attempt("held_is_undefined", () => fr.register(c, undefined));
attempt("token_is_target", () => fr.register(c, "held", c));

// --- an unregister on a subclass shares the parent's bookkeeping ---
class Tracked extends FinalizationRegistry<any> {
  label = "tracked";
}
const sub = new Tracked(() => { calls++; });
sub.register(a, "sub", tok2);
console.log("subclass_unregister=" + sub.unregister(tok2));
console.log("subclass_again=" + sub.unregister(tok2));
console.log("subclass_not_parent=" + fr.unregister(tok2));
console.log("final_callback_calls=" + calls);
