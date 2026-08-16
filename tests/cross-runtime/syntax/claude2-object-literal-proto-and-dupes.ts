// Cross-runtime: what an object LITERAL does beyond storing values — the
// `__proto__: v` form sets the prototype while every other spelling of that
// name makes an ordinary property, duplicate keys are last-wins, and the key
// order is integer-like keys first regardless of where they were written.

const base: any = { inherited: "from-base" };

// 1) `__proto__: v` sets the prototype and creates no own property.
const withProto: any = { __proto__: base, own: 1 };
console.log("proto_is_base=" + (Object.getPrototypeOf(withProto) === base));
console.log("proto_own_keys=" + Object.getOwnPropertyNames(withProto).join(","));
console.log("proto_inherited=" + withProto.inherited);

// 2) A COMPUTED key of the same name is an ordinary own property.
const computedName = "__proto__";
const computed: any = { [computedName]: base, own: 2 };
console.log("computed_proto_is_object=" + (Object.getPrototypeOf(computed) === Object.prototype));
console.log("computed_own_keys=" + Object.getOwnPropertyNames(computed).join(","));

// 3) A shorthand method named `__proto__` is an own property too.
const asMethod: any = { __proto__() { return "method"; }, own: 3 };
console.log("method_proto_is_object=" + (Object.getPrototypeOf(asMethod) === Object.prototype));
console.log("method_proto_call=" + asMethod.__proto__());

// 4) `__proto__: null` leaves an object with no prototype at all.
const bare: any = { __proto__: null, k: "v" };
console.log("bare_proto=" + String(Object.getPrototypeOf(bare)));
console.log("bare_has_hasOwnProperty=" + (typeof bare.hasOwnProperty));
console.log("bare_keys=" + Object.keys(bare).join(","));

// 5) A non-object, non-null `__proto__` value is ignored.
const ignored: any = { __proto__: 42, k: 1 };
console.log("primitive_proto_ignored=" + (Object.getPrototypeOf(ignored) === Object.prototype));
console.log("primitive_proto_own=" + Object.getOwnPropertyNames(ignored).join(","));

// 6) Duplicate data keys: the last one wins and there is only one property.
const dupes: any = { a: 1, b: 2, a: 3 };
console.log("dupes_value=" + dupes.a);
console.log("dupes_keys=" + Object.keys(dupes).join(","));

// 7) The duplicates are still EVALUATED in order — a call in the losing
//    position happens.
const evalOrder: string[] = [];
function note(label: string, v: any): any {
  evalOrder.push(label);
  return v;
}
const dupeEval: any = { x: note("first", 1), y: note("mid", 2), x: note("last", 3) };
console.log("dupe_eval_order=" + evalOrder.join(","));
console.log("dupe_eval_value=" + dupeEval.x);
console.log("dupe_eval_keys=" + Object.keys(dupeEval).join(","));

// 8) A string key and an identifier key that spell the same name are the same
//    property.
const spellings: any = { "k": 1, k: 2 };
console.log("spellings=" + spellings.k + "|" + Object.keys(spellings).length);

// 9) A getter and a setter of the same name merge into one accessor.
const accessorPair: any = {
  get pair(): string { return "got:" + this._v; },
  set pair(v: string) { this._v = v; },
};
accessorPair.pair = "set-value";
console.log("accessor_pair=" + accessorPair.pair);
const pairDesc: any = Object.getOwnPropertyDescriptor(accessorPair, "pair");
console.log("accessor_pair_shape=" + (typeof pairDesc.get) + "/" + (typeof pairDesc.set));

// 10) A data property after an accessor of the same name replaces it entirely.
const replaced: any = {
  get z(): number { return 1; },
  z: 99,
};
const zDesc: any = Object.getOwnPropertyDescriptor(replaced, "z");
console.log("accessor_replaced=" + replaced.z + "|has_get=" + (zDesc.get !== undefined) +
  "|writable=" + zDesc.writable);

// 11) An accessor after a data property replaces it the other way round.
const replacedBack: any = {
  y: 5,
  get y(): number { return 7; },
};
const yDesc: any = Object.getOwnPropertyDescriptor(replacedBack, "y");
console.log("data_replaced=" + replacedBack.y + "|has_get=" + (yDesc.get !== undefined) +
  "|writable=" + String(yDesc.writable));

// 12) Key ORDER: integer-like keys come first, ascending, then string keys in
//     insertion order.
const ordered: any = { b: 1, 2: 2, a: 3, 1: 4, "0": 5 };
console.log("key_order=" + Object.keys(ordered).join(","));

// 13) Keys that only LOOK numeric keep their insertion position.
const nearlyNumeric: any = { "01": 1, "1.5": 2, "-1": 3, "10": 4, "2": 5 };
console.log("nearly_numeric_order=" + Object.keys(nearlyNumeric).join(","));

// 14) Symbol keys never appear in Object.keys, and come last in the own-keys
//     listing.
const sym = Symbol("s");
const withSymbol: any = { [sym]: 1, plain: 2, 3: 3 };
console.log("symbol_in_keys=" + Object.keys(withSymbol).join(","));
console.log("symbol_own_names=" + Object.getOwnPropertyNames(withSymbol).join(","));
console.log("symbol_own_symbols=" + Object.getOwnPropertySymbols(withSymbol).length);

// 15) Shorthand properties take the binding's current value, not a reference.
let shorthandValue = "before";
const shorthand: any = { shorthandValue };
shorthandValue = "after";
console.log("shorthand=" + shorthand.shorthandValue);

// 16) Spread inside a literal is applied in position, so a later plain key
//     overrides a spread one and a later spread overrides an earlier key.
const src: any = { p: "from-spread", q: "q-spread" };
const spreadFirst: any = { ...src, p: "explicit" };
const spreadLast: any = { p: "explicit", ...src };
console.log("spread_first=" + spreadFirst.p + "|" + spreadFirst.q);
console.log("spread_last=" + spreadLast.p);

// 17) Spread copies own ENUMERABLE properties only, and it reads getters.
const getterSource: any = {};
Object.defineProperty(getterSource, "hidden", { value: 1, enumerable: false });
Object.defineProperty(getterSource, "shown", { get() { return "read"; }, enumerable: true });
const spreadCopy: any = { ...getterSource };
console.log("spread_copy_keys=" + Object.keys(spreadCopy).join(","));
const shownDesc: any = Object.getOwnPropertyDescriptor(spreadCopy, "shown");
console.log("spread_copy_is_data=" + (shownDesc.get === undefined) + "|" + spreadCopy.shown);

// 18) A computed key is coerced to a string with the usual rules, and two keys
//     that coerce the same collapse.
const collapsed: any = { [1]: "number-one", ["1"]: "string-one" };
console.log("collapsed=" + collapsed["1"] + "|" + Object.keys(collapsed).length);

// 19) Computed keys are evaluated in source order, interleaved with values.
const keyOrder: string[] = [];
function key(name: string): string {
  keyOrder.push("key:" + name);
  return name;
}
function val(name: string): number {
  keyOrder.push("val:" + name);
  return 1;
}
const interleaved: any = { [key("a")]: val("a"), [key("b")]: val("b") };
console.log("computed_eval_order=" + keyOrder.join(","));
console.log("computed_result_keys=" + Object.keys(interleaved).join(","));

// 20) A literal's properties are writable, enumerable and configurable.
const plain: any = { attr: 1 };
const attrDesc: any = Object.getOwnPropertyDescriptor(plain, "attr");
console.log("literal_attrs=" + attrDesc.writable + "/" + attrDesc.enumerable + "/" + attrDesc.configurable);

// 21) A method in a literal is not enumerable-different from a data property,
//     but it has no `prototype` and is not constructible.
const method: any = { m(): number { return 1; } };
const mDesc: any = Object.getOwnPropertyDescriptor(method, "m");
console.log("method_attrs=" + mDesc.enumerable + "/" + mDesc.writable);
console.log("method_has_prototype=" + Object.prototype.hasOwnProperty.call(method.m, "prototype"));
