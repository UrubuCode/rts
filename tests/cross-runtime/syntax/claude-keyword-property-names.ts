// Cross-runtime: reserved words are legal PROPERTY names — in a literal, after
// a dot, as a method, and as an accessor. `get`/`set` are only special when
// followed by a property name, so `get: 1` and `get()` are ordinary members.

const lit: any = {
  get: 1,
  set: 2,
  new: 3,
  class: 4,
  if: 5,
  else: 6,
  function: 7,
  return: 8,
  typeof: 9,
  delete: 10,
  in: 11,
  of: 12,
  instanceof: 13,
  null: 14,
  true: 15,
  false: 16,
  this: 17,
  super: 18,
  var: 19,
  const: 20,
  let: 21,
  static: 22,
  async: 23,
  await: 24,
  yield: 25,
  default: 26,
  case: 27,
  do: 28,
  for: 29,
  while: 30,
};

console.log("count=" + Object.keys(lit).length);
console.log("order=" + Object.keys(lit).join(","));
console.log("dot_get=" + lit.get);
console.log("dot_class=" + lit.class);
console.log("dot_null=" + lit.null);
console.log("dot_true=" + lit.true);
console.log("dot_this=" + lit.this);
console.log("dot_default=" + lit.default);
console.log("dot_typeof=" + lit.typeof);
console.log("dot_in=" + lit.in);
console.log("sum=" + Object.keys(lit).reduce((a, k) => a + lit[k], 0));

// `get` and `set` as ordinary METHOD names.
const methods: any = {
  get(): string { return "method-get"; },
  set(v: number): string { return "method-set:" + v; },
  new(): string { return "method-new"; },
  delete(): string { return "method-delete"; },
  static(): string { return "method-static"; },
};
console.log("method_get=" + methods.get());
console.log("method_set=" + methods.set(9));
console.log("method_new=" + methods.new());
console.log("method_delete=" + methods.delete());
console.log("method_static=" + methods.static());
console.log("method_names=" + [methods.get.name, methods.set.name, methods.new.name].join(","));

// A real accessor NAMED `get` (and one named `set`).
const accessors: any = {
  _v: 0,
  get get(): string { return "accessor-get:" + this._v; },
  set set(v: number) { this._v = v * 2; },
  get value(): number { return this._v; },
  set value(v: number) { this._v = v + 1; },
};
console.log("accessor_named_get=" + accessors.get);
accessors.set = 21;
console.log("after_setter_named_set=" + accessors._v);
accessors.value = 10;
console.log("after_value_setter=" + accessors.value);

const gd = Object.getOwnPropertyDescriptor(accessors, "get") as any;
console.log("get_is_accessor=" + (typeof gd.get === "function" && gd.set === undefined));
console.log("get_accessor_name=" + gd.get.name);
const sd = Object.getOwnPropertyDescriptor(accessors, "set") as any;
console.log("set_is_accessor=" + (sd.get === undefined && typeof sd.set === "function"));
console.log("set_accessor_name=" + sd.set.name);
const vd = Object.getOwnPropertyDescriptor(accessors, "value") as any;
console.log("value_has_both=" + (typeof vd.get === "function" && typeof vd.set === "function"));

// Shorthand needs a real identifier, so `of`, `async`, `get` and `from` work.
const of = "OF";
const async = "ASYNC";
const get = "GET";
const from = "FROM";
const shorthand = { of, async, get, from };
console.log("shorthand=" + Object.keys(shorthand).map((k) => k + "=" + (shorthand as any)[k]).join(","));

// A class may use reserved words for members too.
class Reserved {
  static default = "static-default";
  if(): string { return "if-method"; }
  get class(): string { return "class-getter"; }
  new(): string { return "new-method"; }
}
const r = new Reserved();
console.log("class_if=" + r.if());
console.log("class_getter=" + r.class);
console.log("class_new=" + r.new());
console.log("class_static_default=" + Reserved.default);

// Numeric and string keys sit beside them; integer-like keys come first.
const mixed: any = { 10: "ten", get: "g", 2: "two", "with space": "ws", 1: "one" };
console.log("mixed_order=" + Object.keys(mixed).join(","));
console.log("mixed_bracket=" + mixed["with space"]);

// Trailing commas are legal in literals, parameter lists and calls.
function trailing(a: number, b: number,): number { return a + b; }
console.log("trailing_params=" + trailing(1, 2,));
console.log("trailing_array=" + [1, 2, 3,].length);
console.log("trailing_object=" + Object.keys({ a: 1, b: 2, }).join(","));

// `delete` on a reserved-word key.
console.log("delete_keyword_key=" + delete lit.class);
console.log("class_gone=" + ("class" in lit));

// `in` with a reserved-word key.
console.log("in_keyword=" + ("typeof" in lit));
console.log("in_missing=" + ("nothing" in lit));
