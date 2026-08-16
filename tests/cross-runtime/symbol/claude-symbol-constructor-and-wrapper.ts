// Cross-runtime: `Symbol` is a constructor you may never `new`, and the object
// WRAPPER a symbol gets when it is boxed — where the two disagree on instanceof,
// typeof, property storage and equality.

const sym = Symbol("boxed");

// --- `new Symbol()` is refused; the plain call is the only way ---
try { new (Symbol as any)(); console.log("new_symbol=no_throw"); }
catch (e: any) { console.log("new_symbol=" + e.constructor.name); }
try { new (Symbol as any)("x"); console.log("new_symbol_arg=no_throw"); }
catch (e: any) { console.log("new_symbol_arg=" + e.constructor.name); }
try { Reflect.construct(Symbol as any, []); console.log("reflect_construct=no_throw"); }
catch (e: any) { console.log("reflect_construct=" + e.constructor.name); }
console.log("plain_call=" + typeof Symbol("ok"));

// --- a primitive symbol is not an instance of anything ---
console.log("prim_instanceof=" + ((sym as any) instanceof Symbol));
console.log("prim_instanceof_object=" + ((sym as any) instanceof Object));
console.log("prim_typeof=" + typeof sym);
console.log("prim_tag=" + Object.prototype.toString.call(sym));

// --- Object(sym) boxes it ---
const box: any = Object(sym);
console.log("box_typeof=" + typeof box);
console.log("box_instanceof=" + (box instanceof Symbol));
console.log("box_tag=" + Object.prototype.toString.call(box));
console.log("box_proto=" + (Object.getPrototypeOf(box) === Symbol.prototype));
console.log("box_valueOf_is_sym=" + (box.valueOf() === sym));
console.log("box_not_sym=" + ((box as any) === (sym as any)));
console.log("box_loose_eq=" + ((box as any) == (sym as any)));
console.log("box_of_box_same=" + (Object(box) === box));
console.log("box_twice_distinct=" + (Object(sym) === Object(sym)));

// --- the box carries the description through the prototype accessor ---
console.log("box_description=" + box.description);
console.log("box_tostring=" + box.toString());
console.log("box_own_names=" + Object.getOwnPropertyNames(box).length);

// --- a boxed symbol is still a valid property KEY only after unboxing ---
const holder: any = {};
holder[box] = "viaBox";
console.log("box_as_key=" + holder[sym]);
console.log("box_key_is_symbol=" + (typeof Reflect.ownKeys(holder)[0]));

// --- extra properties can be put ON the box, and are lost on re-boxing ---
box.extra = 1;
console.log("box_extra=" + box.extra);
console.log("reboxed_extra=" + String(Object(sym).extra));

// --- Symbol.prototype.valueOf brand check ---
function bad(label: string, fn: () => any): void {
  try { const v = fn(); console.log(label + "=no_throw:" + String(typeof v)); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
bad("valueOf_plain", () => Symbol.prototype.valueOf.call({}));
bad("valueOf_string", () => Symbol.prototype.valueOf.call("x"));
bad("valueOf_box", () => Symbol.prototype.valueOf.call(box));
bad("valueOf_prim", () => Symbol.prototype.valueOf.call(sym));

// --- Symbol.prototype is an ordinary object, not a Symbol ---
console.log("proto_typeof=" + typeof Symbol.prototype);
bad("proto_description", () => (Symbol.prototype as any).description);
bad("proto_tostring", () => Symbol.prototype.toString());

// --- Symbol.prototype[Symbol.toPrimitive] unwraps regardless of hint ---
const tp: any = (Symbol.prototype as any)[Symbol.toPrimitive];
console.log("toPrimitive_default=" + (tp.call(box, "default") === sym));
console.log("toPrimitive_string=" + (tp.call(box, "string") === sym));
console.log("toPrimitive_number=" + (tp.call(box, "number") === sym));
console.log("toPrimitive_length=" + tp.length);

// --- a class cannot extend Symbol usefully ---
class SubSymbol extends (Symbol as any) {}
bad("subclass_new", () => new SubSymbol());
console.log("subclass_static_iterator=" + typeof (SubSymbol as any).iterator);

// --- Symbol is not callable as a constructor through .call/.apply either ---
console.log("call_as_function=" + typeof (Symbol as any).call(null, "viaCall"));
console.log("apply_as_function=" + typeof (Symbol as any).apply(null, ["viaApply"]));
console.log("call_description=" + (Symbol as any).call(null, "viaCall").description);
