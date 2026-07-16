// Cross-runtime: Date.prototype[Symbol.toPrimitive] hint dispatch.
// Only the "number" hint is checked for a value (string/default hints produce
// local-timezone text, so we assert only on their TYPE, never their content).

const d = new Date(Date.UTC(2024, 0, 15, 10, 30, 45, 123));
const prim = (d as any)[Symbol.toPrimitive];

console.log("has_toPrimitive=" + (typeof prim === "function"));
console.log("is_own_of_proto=" + Object.prototype.hasOwnProperty.call(Date.prototype, Symbol.toPrimitive));
console.log("length=" + prim.length);
console.log("name=" + prim.name);

// hint "number" => the timestamp.
console.log("number_hint=" + prim.call(d, "number"));
console.log("number_hint_is_time=" + (prim.call(d, "number") === d.getTime()));
console.log("number_hint_type=" + typeof prim.call(d, "number"));

// hint "default" => string (Date is the one built-in where default != number).
console.log("default_hint_type=" + typeof prim.call(d, "default"));
console.log("string_hint_type=" + typeof prim.call(d, "string"));

// default and string hints agree with each other (both go to toString).
console.log("default_eq_string=" + (prim.call(d, "default") === prim.call(d, "string")));
console.log("default_eq_toString=" + (prim.call(d, "default") === d.toString()));

// An invalid hint throws TypeError.
try {
  prim.call(d, "bogus");
  console.log("bad_hint_threw=false");
} catch (e) {
  console.log("bad_hint_threw=true");
  console.log("bad_hint_name=" + (e as Error).name);
}
try {
  prim.call(d, undefined);
  console.log("undef_hint_threw=false");
} catch (e) {
  console.log("undef_hint_threw=true");
  console.log("undef_hint_name=" + (e as Error).name);
}

// Operators route through the hints:
// unary + and Number() use "number"; + concat and template use "default".
console.log("plus_is_number=" + (+d === d.getTime()));
console.log("Number_is_number=" + (Number(d) === d.getTime()));
console.log("concat_is_string=" + (typeof ((d as any) + "") === "string"));
console.log("concat_eq_toString=" + ((d as any) + "" === d.toString()));
console.log("template_eq_toString=" + (`${d}` === d.toString()));
console.log("String_eq_toString=" + (String(d) === d.toString()));

// Subtraction is a numeric op => "number" hint.
console.log("minus_numeric=" + ((d as any) - 0 === d.getTime()));

// Invalid Date: number hint is NaN.
const bad = new Date(NaN);
console.log("invalid_number_hint_nan=" + Number.isNaN(prim.call(bad, "number")));
console.log("invalid_string_hint=" + prim.call(bad, "string"));
