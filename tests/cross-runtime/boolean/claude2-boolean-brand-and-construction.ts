// ONE thing: Boolean called versus constructed, and the brand check that
// separates them. Boolean.prototype is itself a Boolean object holding false,
// so valueOf works on it — while any other receiver is a TypeError, wrapper of
// a different type included.

function attempt(label: string, fn: () => any): void {
  try {
    console.log(label + "=" + String(fn()));
  } catch (e) {
    console.log(label + "!" + (e as any).constructor.name);
  }
}

// --- the constructor object ---
console.log("typeof=" + typeof Boolean);
console.log("name=" + Boolean.name + " length=" + Boolean.length);
console.log("keys=[" + Object.keys(Boolean).join(",") + "]");
console.log("proto_is_Function=" + (Object.getPrototypeOf(Boolean) === Function.prototype));
const pd = Object.getOwnPropertyDescriptor(Boolean, "prototype") as any;
console.log("prototype_flags=" + [pd.writable, pd.enumerable, pd.configurable].join(","));
console.log("constructor_identity=" + (Boolean.prototype.constructor === Boolean));
console.log("proto_keys=[" + Object.keys(Boolean.prototype).join(",") + "]");
const methods: string[] = ["toString", "valueOf"];
for (const m of methods) {
  const fn = (Boolean.prototype as any)[m];
  console.log("method_" + m + "=" + typeof fn + ":" + String(fn.length) + ":" + String(fn.name));
}

// --- Boolean.prototype is a Boolean object whose data is false ---
console.log("proto_typeof=" + typeof Boolean.prototype);
console.log("proto_tag=" + Object.prototype.toString.call(Boolean.prototype));
console.log("proto_valueOf=" + String(Boolean.prototype.valueOf.call(Boolean.prototype)));
console.log("proto_toString=" + Boolean.prototype.toString.call(Boolean.prototype));
console.log("proto_is_truthy=" + (Boolean.prototype ? "truthy" : "falsy"));
console.log("proto_of_proto=" + (Object.getPrototypeOf(Boolean.prototype) === Object.prototype));

// --- called: a primitive. constructed: an object that is always truthy ---
console.log("call_typeof=" + typeof Boolean(0) + " value=" + Boolean(0));
console.log("call_no_args=" + Boolean() + " typeof=" + typeof Boolean());
console.log("new_typeof=" + typeof new Boolean(false));
console.log("new_tag=" + Object.prototype.toString.call(new Boolean(false)));
console.log("new_truthy=" + (new Boolean(false) ? "truthy" : "falsy"));
console.log("new_valueOf=" + new Boolean(false).valueOf());
console.log("new_toString=" + new Boolean(false).toString());
console.log("new_no_args=" + new Boolean().valueOf());
console.log("new_equalities=" + (new Boolean(false) == false) + "," + ((new Boolean(false) as any) === false));
console.log("two_wrappers=" + (new Boolean(true) == new Boolean(true)));
console.log("instanceof=" + (new Boolean(true) instanceof Boolean) + "," + ((true as any) instanceof Boolean));
console.log("reflect_construct=" + String(Reflect.construct(Boolean, [1]).valueOf()));

// --- the wrapper in coercing positions: valueOf answers, so it can be false ---
const falseBox: any = new Boolean(false);
console.log("box_plus=" + String(falseBox + 0));
console.log("box_template=" + `${falseBox}`);
console.log("box_json=" + JSON.stringify({ b: falseBox }));
console.log("box_json_alone=" + String(JSON.stringify(falseBox)));
console.log("box_loose_zero=" + (falseBox == 0) + " loose_empty=" + (falseBox == ""));
console.log("box_negated=" + !falseBox);
console.log("box_and=" + String(falseBox && "reached"));
console.log("box_nullish=" + String(falseBox ?? "never"));
console.log("box_keys=" + Object.keys(falseBox).length);
console.log("box_extra_property=" + (() => {
  const b: any = new Boolean(true);
  Reflect.set(b, "note", "kept");
  return b.note + ":" + Object.keys(b).join(",");
})());

// --- brand checks on the prototype methods ---
attempt("valueOf_true", () => Boolean.prototype.valueOf.call(true));
attempt("valueOf_false", () => Boolean.prototype.valueOf.call(false));
attempt("valueOf_wrapper", () => Boolean.prototype.valueOf.call(new Boolean(true)));
attempt("valueOf_number", () => Boolean.prototype.valueOf.call(1));
attempt("valueOf_number_wrapper", () => Boolean.prototype.valueOf.call(new Number(1)));
attempt("valueOf_string_wrapper", () => Boolean.prototype.valueOf.call(new String("true")));
attempt("valueOf_object", () => Boolean.prototype.valueOf.call({}));
attempt("valueOf_null", () => Boolean.prototype.valueOf.call(null));
attempt("valueOf_undefined", () => Boolean.prototype.valueOf.call(undefined));
attempt("valueOf_no_receiver", () => (0, Boolean.prototype.valueOf)());
attempt("toString_true", () => Boolean.prototype.toString.call(true));
attempt("toString_object", () => Boolean.prototype.toString.call({}));
attempt("toString_faked_slot", () => Boolean.prototype.toString.call({ valueOf: () => true }));

// --- a detached method still works when applied to a primitive ---
const detached = Boolean.prototype.toString;
console.log("detached_apply=" + detached.apply(true) + "," + detached.call(false));
console.log("reflect_apply=" + Reflect.apply(Boolean.prototype.valueOf, new Boolean(true), []));

// --- a subclass keeps the internal slot and its own prototype chain ---
class Flag extends Boolean {
  describe(): string {
    return this.valueOf() ? "on" : "off";
  }
}
const on = new Flag(true);
const off = new Flag(0);
console.log("subclass_values=" + on.valueOf() + "," + off.valueOf());
console.log("subclass_describe=" + on.describe() + "," + off.describe());
console.log("subclass_tag=" + Object.prototype.toString.call(on));
console.log("subclass_truthy=" + (off ? "truthy" : "falsy"));
console.log("subclass_instanceof=" + (on instanceof Flag) + "," + (on instanceof Boolean));
console.log("subclass_toString=" + on.toString() + "," + off.toString());
console.log("subclass_json=" + JSON.stringify({ f: off }));

// --- Boolean() is not a coercion of the ARGUMENT's valueOf ---
const trap: any = {
  valueOf() {
    return false;
  },
};
console.log("Boolean_of_trap=" + Boolean(trap));
console.log("Boolean_of_wrapper=" + Boolean(new Boolean(false)));
console.log("Boolean_of_boxed_zero=" + Boolean(new Number(0)));
console.log("Boolean_argument_count=" + Boolean(false, true as any));
