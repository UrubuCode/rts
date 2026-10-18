// Cross-runtime: Buffer.from(object) precedence — valueOf() yielding a string or
// object comes FIRST, then the array-like form (length / numeric keys, ToUint8
// wrap, getters in ascending order), then the JSON {type:'Buffer', data} form,
// then a Symbol.toPrimitive string, else ERR_INVALID_ARG_TYPE.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name + ":" + e.code;
  }
};
const hex = (b: any) => b.toString("hex");

console.log("arraylike=" + t(() => hex(Buffer.from({ length: 2, 0: 5, 1: 6 } as any))));
console.log("missing_keys=" + t(() => hex(Buffer.from({ length: 3, 0: 1 } as any))));
console.log("length_zero=" + t(() => hex(Buffer.from({ length: 0 } as any))));
console.log("length_negative=" + t(() => hex(Buffer.from({ length: -1 } as any))));
console.log("length_string_is_empty=" + t(() => hex(Buffer.from({ length: "2", 0: 7 } as any))));
console.log("length_nan=" + t(() => hex(Buffer.from({ length: NaN, 0: 7 } as any))));
console.log("empty_array=" + t(() => hex(Buffer.from([]))));
console.log("no_length=" + t(() => hex(Buffer.from({ a: 1 } as any))));
console.log("wrap=" + t(() => hex(Buffer.from({ length: 4, 0: 257, 1: -1, 2: 1.9, 3: "7" } as any))));
console.log("non_finite=" + t(() => hex(Buffer.from({ length: 3, 0: NaN, 1: Infinity, 2: "x" } as any))));

const order: string[] = [];
const getters: any = { length: 3 };
for (let i = 0; i < 3; i++) {
  Object.defineProperty(getters, i, { get: function () { order.push("g" + i); return i + 1; }, enumerable: true });
}
console.log("getters=" + t(() => hex(Buffer.from(getters))) + " " + order.join(">"));

console.log("valueOf_string=" + t(() => hex(Buffer.from({ valueOf() { return "hi"; } } as any))));
console.log("valueOf_beats_arraylike=" + t(() => hex(Buffer.from({ valueOf() { return "hi"; }, length: 1, 0: 9 } as any))));
console.log("valueOf_object=" + t(() => hex(Buffer.from({ valueOf() { return { length: 1, 0: 9 }; } } as any))));
console.log("valueOf_hex_encoding=" + t(() => hex(Buffer.from({ valueOf() { return "6869"; } } as any, "hex"))));
console.log("valueOf_number_refused=" + t(() => hex(Buffer.from({ valueOf() { return 5; } } as any))));
console.log("valueOf_number_then_arraylike=" + t(() => hex(Buffer.from({ valueOf() { return 5; }, length: 1, 0: 3 } as any))));
console.log("valueOf_throws=" + t(() => hex(Buffer.from({ valueOf() { throw new RangeError("boom"); } } as any))));

console.log("toPrimitive_string=" + t(() => hex(Buffer.from({ [Symbol.toPrimitive]() { return "ok"; } } as any))));
console.log("arraylike_beats_toPrimitive=" + t(() => hex(Buffer.from({ length: 1, 0: 8, [Symbol.toPrimitive]() { return "ok"; } } as any))));
console.log("toPrimitive_number_refused=" + t(() => hex(Buffer.from({ [Symbol.toPrimitive]() { return 1; } } as any))));

console.log("json_form=" + t(() => hex(Buffer.from({ type: "Buffer", data: [1, 2, 3] } as any))));
console.log("json_roundtrip=" + t(() => hex(Buffer.from(JSON.parse(JSON.stringify(Buffer.from("abc")))))));
console.log("empty_object_refused=" + t(() => hex(Buffer.from({} as any))));
