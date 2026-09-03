// node:util — util.types.* brand predicates. Every implemented predicate is
// tested in BOTH directions (true for its own kind, false for the neighbours
// it is most likely to be confused with by a mis-wired instance_of). Every
// absent predicate is tested for the shape of its absence: this project's
// rule is that a missing name answers `undefined` at the call site (throws
// loud), never a silently-false function. Node answers verified with a real
// `node -e` (v20.19.5) alongside this file's own comments.
import { describe, test, expect } from "rts:test";
import util from "node:util";

const types: any = util.types;

// ---------------------------------------------------------------------------
// Collections
// ---------------------------------------------------------------------------
const isMap_map = types.isMap(new Map());
const isMap_obj = types.isMap({});
const isMap_set = types.isMap(new Set());
const isSet_set = types.isSet(new Set());
const isSet_map = types.isSet(new Map());
const isWeakMap_wm = types.isWeakMap(new WeakMap());
const isWeakMap_map = types.isWeakMap(new Map());
const isWeakSet_ws = types.isWeakSet(new WeakSet());
const isWeakSet_set = types.isWeakSet(new Set());

// ---------------------------------------------------------------------------
// Date / RegExp / Promise
// ---------------------------------------------------------------------------
const isDate_date = types.isDate(new Date());
const isDate_obj = types.isDate({});
const isRegExp_re = types.isRegExp(/x/);
const isRegExp_str = types.isRegExp("x");
const isPromise_p = types.isPromise(Promise.resolve(1));
const isPromise_obj = types.isPromise({ then() {} });

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------
const isNativeError_type = types.isNativeError(new TypeError("x"));
const isNativeError_range = types.isNativeError(new RangeError("x"));
const isNativeError_plain = types.isNativeError(new Error("x"));
const isNativeError_obj = types.isNativeError({});
const isNativeError_str = types.isNativeError("Error");

// ---------------------------------------------------------------------------
// ArrayBuffer family
// ---------------------------------------------------------------------------
const ab = new ArrayBuffer(8);
const isArrayBuffer_ab = types.isArrayBuffer(ab);
const isArrayBuffer_view = types.isArrayBuffer(new Uint8Array(ab));
const isAnyArrayBuffer_ab = types.isAnyArrayBuffer(ab);
const isDataView_dv = types.isDataView(new DataView(ab));
const isDataView_ab = types.isDataView(ab);
const isDataView_ta = types.isDataView(new Uint8Array(ab));

// ---------------------------------------------------------------------------
// Typed arrays — every kind against a neighbour it must NOT match
// ---------------------------------------------------------------------------
const isInt8Array_own = types.isInt8Array(new Int8Array(1));
const isInt8Array_u8 = types.isInt8Array(new Uint8Array(1));
const isUint8Array_own = types.isUint8Array(new Uint8Array(1));
const isUint8Array_i8 = types.isUint8Array(new Int8Array(1));
const isUint8ClampedArray_own = types.isUint8ClampedArray(new Uint8ClampedArray(1));
const isUint8ClampedArray_u8 = types.isUint8ClampedArray(new Uint8Array(1));
const isInt16Array_own = types.isInt16Array(new Int16Array(1));
const isInt16Array_u16 = types.isInt16Array(new Uint16Array(1));
const isUint16Array_own = types.isUint16Array(new Uint16Array(1));
const isUint16Array_i16 = types.isUint16Array(new Int16Array(1));
const isInt32Array_own = types.isInt32Array(new Int32Array(1));
const isInt32Array_u32 = types.isInt32Array(new Uint32Array(1));
const isUint32Array_own = types.isUint32Array(new Uint32Array(1));
const isUint32Array_i32 = types.isUint32Array(new Int32Array(1));
const isFloat32Array_own = types.isFloat32Array(new Float32Array(1));
const isFloat32Array_f64 = types.isFloat32Array(new Float64Array(1));
const isFloat64Array_own = types.isFloat64Array(new Float64Array(1));
const isFloat64Array_f32 = types.isFloat64Array(new Float32Array(1));
const isBigInt64Array_own = types.isBigInt64Array(new BigInt64Array(1));
const isBigInt64Array_biguint = types.isBigInt64Array(new BigUint64Array(1));
const isBigUint64Array_own = types.isBigUint64Array(new BigUint64Array(1));
const isBigUint64Array_bigint = types.isBigUint64Array(new BigInt64Array(1));

const isTypedArray_i8 = types.isTypedArray(new Int8Array(1));
const isTypedArray_f64 = types.isTypedArray(new Float64Array(1));
const isTypedArray_dv = types.isTypedArray(new DataView(ab));
const isTypedArray_plain = types.isTypedArray([1, 2, 3]);

const isArrayBufferView_ta = types.isArrayBufferView(new Uint8Array(1));
const isArrayBufferView_dv = types.isArrayBufferView(new DataView(ab));
const isArrayBufferView_ab = types.isArrayBufferView(ab);
const isArrayBufferView_plain = types.isArrayBufferView([1, 2, 3]);

// ---------------------------------------------------------------------------
// Boxed primitives
// ---------------------------------------------------------------------------
const isBooleanObject_box = types.isBooleanObject(new Boolean(true));
const isBooleanObject_prim = types.isBooleanObject(true);
const isNumberObject_box = types.isNumberObject(new Number(1));
const isNumberObject_prim = types.isNumberObject(1);
const isStringObject_box = types.isStringObject(new String("x"));
const isStringObject_prim = types.isStringObject("x");
const isSymbolObject_box = types.isSymbolObject(Object(Symbol("x")));
const isSymbolObject_prim = types.isSymbolObject(Symbol("x"));
const isBigIntObject_box = types.isBigIntObject(Object(1n));
const isBigIntObject_prim = types.isBigIntObject(1n);

const isBoxedPrimitive_num = types.isBoxedPrimitive(new Number(1));
const isBoxedPrimitive_str = types.isBoxedPrimitive(new String("x"));
const isBoxedPrimitive_bool = types.isBoxedPrimitive(new Boolean(false));
const isBoxedPrimitive_sym = types.isBoxedPrimitive(Object(Symbol("x")));
const isBoxedPrimitive_bigint = types.isBoxedPrimitive(Object(1n));
const isBoxedPrimitive_prim_num = types.isBoxedPrimitive(1);
const isBoxedPrimitive_prim_str = types.isBoxedPrimitive("x");
const isBoxedPrimitive_plain_obj = types.isBoxedPrimitive({});

// ---------------------------------------------------------------------------
// Generator objects
// ---------------------------------------------------------------------------
function* gen() {
    yield 1;
}
const isGeneratorObject_gen = types.isGeneratorObject(gen());
const isGeneratorObject_obj = types.isGeneratorObject({ next() {} });
const isGeneratorObject_fn = types.isGeneratorObject(gen);

// ---------------------------------------------------------------------------
// isArray / isModuleNamespaceObject — legacy members already carried on the
// namespace, exercised here alongside the new ones.
// ---------------------------------------------------------------------------
const isArray_arr = types.isArray([1, 2]);
const isArray_obj = types.isArray({ length: 0 });

// ---------------------------------------------------------------------------
// Absent predicates — this project's rule: a missing name throws loud
// (`undefined` is not callable) rather than silently answering `false`.
// Each is probed through a try/catch so one absence does not stop the rest of
// the file, and the OUTCOME (threw vs. answered a boolean) is what the test
// below asserts.
// ---------------------------------------------------------------------------
function probeAbsent(name: string, arg: unknown): "absent" | "threw" | "false" | "true" | "other" {
    const fn = (types as any)[name];
    if (typeof fn !== "function") return "absent";
    try {
        const result = fn(arg);
        if (result === false) return "false";
        if (result === true) return "true";
        return "other";
    } catch {
        return "threw";
    }
}

const isProxyOutcome = probeAbsent("isProxy", new Proxy({}, {}));
const isArgumentsObjectOutcome = probeAbsent("isArgumentsObject", (function () { return arguments; })());
const isAsyncFunctionOutcome = probeAbsent("isAsyncFunction", async function () {});
const isGeneratorFunctionOutcome = probeAbsent("isGeneratorFunction", gen);
const isMapIteratorOutcome = probeAbsent("isMapIterator", new Map().keys());
const isSetIteratorOutcome = probeAbsent("isSetIterator", new Set().keys());
const isExternalOutcome = probeAbsent("isExternal", {});
const isCryptoKeyOutcome = probeAbsent("isCryptoKey", {});
const isKeyObjectOutcome = probeAbsent("isKeyObject", {});

describe("util.types — collections", () => {
    test("isMap(new Map()) is true", () => expect(isMap_map).toBe(true));
    test("isMap({}) is false", () => expect(isMap_obj).toBe(false));
    test("isMap(new Set()) is false", () => expect(isMap_set).toBe(false));
    test("isSet(new Set()) is true", () => expect(isSet_set).toBe(true));
    test("isSet(new Map()) is false", () => expect(isSet_map).toBe(false));
    test("isWeakMap(new WeakMap()) is true", () => expect(isWeakMap_wm).toBe(true));
    test("isWeakMap(new Map()) is false", () => expect(isWeakMap_map).toBe(false));
    test("isWeakSet(new WeakSet()) is true", () => expect(isWeakSet_ws).toBe(true));
    test("isWeakSet(new Set()) is false", () => expect(isWeakSet_set).toBe(false));
});

describe("util.types — date / regexp / promise", () => {
    test("isDate(new Date()) is true", () => expect(isDate_date).toBe(true));
    test("isDate({}) is false", () => expect(isDate_obj).toBe(false));
    test("isRegExp(/x/) is true", () => expect(isRegExp_re).toBe(true));
    test("isRegExp('x') is false", () => expect(isRegExp_str).toBe(false));
    test("isPromise(Promise.resolve(1)) is true", () => expect(isPromise_p).toBe(true));
    test("isPromise(thenable) is false", () => expect(isPromise_obj).toBe(false));
});

describe("util.types — errors", () => {
    test("isNativeError(new TypeError()) is true", () => expect(isNativeError_type).toBe(true));
    test("isNativeError(new RangeError()) is true", () => expect(isNativeError_range).toBe(true));
    test("isNativeError(new Error()) is true", () => expect(isNativeError_plain).toBe(true));
    test("isNativeError({}) is false", () => expect(isNativeError_obj).toBe(false));
    test("isNativeError('Error') is false", () => expect(isNativeError_str).toBe(false));
});

describe("util.types — ArrayBuffer family", () => {
    test("isArrayBuffer(ArrayBuffer) is true", () => expect(isArrayBuffer_ab).toBe(true));
    test("isArrayBuffer(view) is false", () => expect(isArrayBuffer_view).toBe(false));
    test("isAnyArrayBuffer(ArrayBuffer) is true", () => expect(isAnyArrayBuffer_ab).toBe(true));
    test("isDataView(DataView) is true", () => expect(isDataView_dv).toBe(true));
    test("isDataView(ArrayBuffer) is false", () => expect(isDataView_ab).toBe(false));
    test("isDataView(typed array) is false", () => expect(isDataView_ta).toBe(false));
});

describe("util.types — typed arrays, own kind vs. a neighbour", () => {
    test("isInt8Array(Int8Array) true", () => expect(isInt8Array_own).toBe(true));
    test("isInt8Array(Uint8Array) false", () => expect(isInt8Array_u8).toBe(false));
    test("isUint8Array(Uint8Array) true", () => expect(isUint8Array_own).toBe(true));
    test("isUint8Array(Int8Array) false", () => expect(isUint8Array_i8).toBe(false));
    test("isUint8ClampedArray(own) true", () => expect(isUint8ClampedArray_own).toBe(true));
    test("isUint8ClampedArray(Uint8Array) false", () => expect(isUint8ClampedArray_u8).toBe(false));
    test("isInt16Array(own) true", () => expect(isInt16Array_own).toBe(true));
    test("isInt16Array(Uint16Array) false", () => expect(isInt16Array_u16).toBe(false));
    test("isUint16Array(own) true", () => expect(isUint16Array_own).toBe(true));
    test("isUint16Array(Int16Array) false", () => expect(isUint16Array_i16).toBe(false));
    test("isInt32Array(own) true", () => expect(isInt32Array_own).toBe(true));
    test("isInt32Array(Uint32Array) false", () => expect(isInt32Array_u32).toBe(false));
    test("isUint32Array(own) true", () => expect(isUint32Array_own).toBe(true));
    test("isUint32Array(Int32Array) false", () => expect(isUint32Array_i32).toBe(false));
    test("isFloat32Array(own) true", () => expect(isFloat32Array_own).toBe(true));
    test("isFloat32Array(Float64Array) false", () => expect(isFloat32Array_f64).toBe(false));
    test("isFloat64Array(own) true", () => expect(isFloat64Array_own).toBe(true));
    test("isFloat64Array(Float32Array) false", () => expect(isFloat64Array_f32).toBe(false));
    test("isBigInt64Array(own) true", () => expect(isBigInt64Array_own).toBe(true));
    test("isBigInt64Array(BigUint64Array) false", () => expect(isBigInt64Array_biguint).toBe(false));
    test("isBigUint64Array(own) true", () => expect(isBigUint64Array_own).toBe(true));
    test("isBigUint64Array(BigInt64Array) false", () => expect(isBigUint64Array_bigint).toBe(false));

    test("isTypedArray(Int8Array) true", () => expect(isTypedArray_i8).toBe(true));
    test("isTypedArray(Float64Array) true", () => expect(isTypedArray_f64).toBe(true));
    test("isTypedArray(DataView) false", () => expect(isTypedArray_dv).toBe(false));
    test("isTypedArray(plain array) false", () => expect(isTypedArray_plain).toBe(false));

    test("isArrayBufferView(typed array) true", () => expect(isArrayBufferView_ta).toBe(true));
    test("isArrayBufferView(DataView) true", () => expect(isArrayBufferView_dv).toBe(true));
    test("isArrayBufferView(ArrayBuffer) false", () => expect(isArrayBufferView_ab).toBe(false));
    test("isArrayBufferView(plain array) false", () => expect(isArrayBufferView_plain).toBe(false));
});

describe("util.types — boxed primitives, box vs. its own primitive", () => {
    test("isBooleanObject(new Boolean) true", () => expect(isBooleanObject_box).toBe(true));
    test("isBooleanObject(true) false", () => expect(isBooleanObject_prim).toBe(false));
    test("isNumberObject(new Number) true", () => expect(isNumberObject_box).toBe(true));
    test("isNumberObject(1) false", () => expect(isNumberObject_prim).toBe(false));
    test("isStringObject(new String) true", () => expect(isStringObject_box).toBe(true));
    test("isStringObject('x') false", () => expect(isStringObject_prim).toBe(false));
    test("isSymbolObject(Object(Symbol())) true", () => expect(isSymbolObject_box).toBe(true));
    test("isSymbolObject(Symbol()) false", () => expect(isSymbolObject_prim).toBe(false));
    test("isBigIntObject(Object(1n)) true", () => expect(isBigIntObject_box).toBe(true));
    test("isBigIntObject(1n) false", () => expect(isBigIntObject_prim).toBe(false));

    test("isBoxedPrimitive(new Number) true", () => expect(isBoxedPrimitive_num).toBe(true));
    test("isBoxedPrimitive(new String) true", () => expect(isBoxedPrimitive_str).toBe(true));
    test("isBoxedPrimitive(new Boolean) true", () => expect(isBoxedPrimitive_bool).toBe(true));
    test("isBoxedPrimitive(Object(Symbol())) true", () => expect(isBoxedPrimitive_sym).toBe(true));
    test("isBoxedPrimitive(Object(1n)) true", () => expect(isBoxedPrimitive_bigint).toBe(true));
    test("isBoxedPrimitive(1) false", () => expect(isBoxedPrimitive_prim_num).toBe(false));
    test("isBoxedPrimitive('x') false", () => expect(isBoxedPrimitive_prim_str).toBe(false));
    test("isBoxedPrimitive({}) false", () => expect(isBoxedPrimitive_plain_obj).toBe(false));
});

describe("util.types — generator objects", () => {
    test("isGeneratorObject(gen()) true", () => expect(isGeneratorObject_gen).toBe(true));
    test("isGeneratorObject({next(){}}) false", () => expect(isGeneratorObject_obj).toBe(false));
    test("isGeneratorObject(gen itself, undeclared) false", () => expect(isGeneratorObject_fn).toBe(false));
});

describe("util.types — legacy members", () => {
    test("isArray([1,2]) true", () => expect(isArray_arr).toBe(true));
    test("isArray({length:0}) false", () => expect(isArray_obj).toBe(false));
});

describe("util.types — absent predicates must fail loud, not lie quiet", () => {
    // Node: every one of these is a real function. This engine's own doc
    // (util/types.rs) says each is absent by design. The project rule is that
    // an absent member reads as `undefined` at the property access — calling
    // it then throws "is not a function" — which is what `probeAbsent` above
    // reports as "threw". If any of these instead reports "false", that is a
    // hollow predicate: a name that exists and lies rather than one that is
    // honestly missing, and IS a defect per this project's own stated rule.
    test("isProxy is absent (not a hollow false)", () => expect(isProxyOutcome).toBe("absent"));
    test("isArgumentsObject is absent (not a hollow false)", () => expect(isArgumentsObjectOutcome).toBe("absent"));
    test("isAsyncFunction is absent (not a hollow false)", () => expect(isAsyncFunctionOutcome).toBe("absent"));
    test("isGeneratorFunction is absent (not a hollow false)", () => expect(isGeneratorFunctionOutcome).toBe("absent"));
    test("isMapIterator is absent (not a hollow false)", () => expect(isMapIteratorOutcome).toBe("absent"));
    test("isSetIterator is absent (not a hollow false)", () => expect(isSetIteratorOutcome).toBe("absent"));
    test("isExternal is absent (not a hollow false)", () => expect(isExternalOutcome).toBe("absent"));
    test("isCryptoKey is absent (not a hollow false)", () => expect(isCryptoKeyOutcome).toBe("absent"));
    test("isKeyObject is absent (not a hollow false)", () => expect(isKeyObjectOutcome).toBe("absent"));
});
