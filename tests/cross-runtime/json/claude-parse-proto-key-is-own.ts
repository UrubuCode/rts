// Cross-runtime: a "__proto__" member in parsed JSON becomes an ORDINARY OWN
// data property — JSON.parse uses CreateDataProperty, not the Object.prototype
// setter that an object literal or an assignment would go through.

const parsed: any = JSON.parse('{"__proto__":{"polluted":true},"ok":1}');

// --- it is an own, enumerable, writable, configurable data property ---
console.log("has_own=" + Object.prototype.hasOwnProperty.call(parsed, "__proto__"));
console.log("keys=" + Object.keys(parsed).join(","));
console.log("ownNames=" + Object.getOwnPropertyNames(parsed).join(","));
const d: any = Object.getOwnPropertyDescriptor(parsed, "__proto__");
console.log("desc_is_data=" + (d.get === undefined && d.set === undefined));
console.log("desc_flags=" + d.writable + ":" + d.enumerable + ":" + d.configurable);
console.log("desc_value=" + JSON.stringify(d.value));

// --- the prototype was NOT changed ---
console.log("proto_unchanged=" + (Object.getPrototypeOf(parsed) === Object.prototype));
console.log("not_polluted=" + (parsed.polluted === undefined));
console.log("Object_proto_clean=" + (({} as any).polluted === undefined));
console.log("sibling_key=" + parsed.ok);
console.log("roundtrip=" + JSON.stringify(parsed));

// --- contrast: an object literal DOES set the prototype ---
const literal: any = { __proto__: { fromLiteral: true }, ok: 1 };
console.log("literal_own=" + Object.prototype.hasOwnProperty.call(literal, "__proto__"));
console.log("literal_keys=" + Object.keys(literal).join(","));
console.log("literal_inherits=" + literal.fromLiteral);
console.log("literal_proto_is_object=" + (Object.getPrototypeOf(literal) === Object.prototype));

// --- contrast: assignment goes through the Object.prototype setter ---
const assigned: any = {};
assigned["__proto__"] = { fromAssign: true };
console.log("assign_own=" + Object.prototype.hasOwnProperty.call(assigned, "__proto__"));
console.log("assign_inherits=" + assigned.fromAssign);

// --- a primitive __proto__ value from JSON is still an own property ---
const primProto: any = JSON.parse('{"__proto__":42}');
console.log("prim_own=" + Object.prototype.hasOwnProperty.call(primProto, "__proto__"));
console.log("prim_value=" + primProto["__proto__"]);
console.log("prim_typeof=" + typeof primProto["__proto__"]);

// --- but assignment of a primitive to __proto__ is silently ignored ---
const primAssign: any = {};
primAssign["__proto__"] = 42;
console.log("prim_assign_own=" + Object.prototype.hasOwnProperty.call(primAssign, "__proto__"));
console.log("prim_assign_proto=" + (Object.getPrototypeOf(primAssign) === Object.prototype));

// --- nested, and inside an array ---
const nested: any = JSON.parse('{"a":{"__proto__":{"x":1}}}');
console.log("nested_own=" + Object.prototype.hasOwnProperty.call(nested.a, "__proto__"));
console.log("nested_proto=" + (Object.getPrototypeOf(nested.a) === Object.prototype));
const inArray: any = JSON.parse('[{"__proto__":{"y":2}}]');
console.log("in_array_own=" + Object.prototype.hasOwnProperty.call(inArray[0], "__proto__"));

// --- the REVIVER sees "__proto__" as an ordinary key ---
const seen: string[] = [];
const revived: any = JSON.parse('{"__proto__":{"z":3},"b":1}', function (k: any, v: any) {
  if (k !== "") seen.push(k);
  return v;
});
console.log("reviver_keys=" + seen.join(","));
console.log("revived_own=" + Object.prototype.hasOwnProperty.call(revived, "__proto__"));

// --- a reviver returning undefined for it removes the own property ---
const dropped: any = JSON.parse('{"__proto__":{"z":3},"b":1}', function (k: any, v: any) {
  return k === "__proto__" ? undefined : v;
});
console.log("dropped_keys=" + Object.keys(dropped).join(","));
console.log("dropped_own=" + Object.prototype.hasOwnProperty.call(dropped, "__proto__"));

// --- "constructor" and "prototype" are ordinary too ---
const ctor: any = JSON.parse('{"constructor":{"prototype":{"hacked":true}}}');
console.log("ctor_own=" + Object.prototype.hasOwnProperty.call(ctor, "constructor"));
console.log("ctor_value=" + JSON.stringify(ctor.constructor));
console.log("real_ctor_intact=" + (({} as any).constructor === Object));

// --- stringify puts it straight back out ---
console.log("stringify_own_proto=" + JSON.stringify(parsed));
console.log("stringify_literal=" + JSON.stringify(literal));
console.log("global_object_still_clean=" + (({} as any).polluted === undefined) + ":" + (([] as any).polluted === undefined));
