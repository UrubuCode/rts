// Cross-runtime: Object.hasOwn coerces its key like hasOwnProperty does —
// numbers, array indices, typed-array elements, symbols, accessors — and the
// typed-array geometry names stay inherited (not own).

const sym = Symbol("s");
const withGetter = { get g() { return 1; }, [sym]: 2 };
const arr = [1, 2];
const ta = new Uint8Array([9, 8]);
const plain: any = { 0: "a", a: 1 };

const rows: string[] = [];
const both = (label: string, o: any, k: any) => {
  rows.push(label + " " + Object.hasOwn(o, k) + " " + Object.prototype.hasOwnProperty.call(o, k));
};

both("plain 0", plain, 0);
both("plain '0'", plain, "0");
both("plain a", plain, "a");
both("plain b", plain, "b");
both("plain toString", plain, "toString");
both("arr 0", arr, 0);
both("arr 1", arr, 1);
both("arr 2", arr, 2);
both("arr '0'", arr, "0");
both("arr '1'", arr, "1");
both("arr '2'", arr, "2");
both("arr length", arr, "length");
both("arr push", arr, "push");
both("ta 0", ta, 0);
both("ta 1", ta, 1);
both("ta 2", ta, 2);
both("ta '0'", ta, "0");
both("ta -1", ta, -1);
both("ta length", ta, "length");
both("ta byteLength", ta, "byteLength");
both("ta byteOffset", ta, "byteOffset");
both("ta buffer", ta, "buffer");
both("sym", withGetter, sym);
both("other sym", withGetter, Symbol("s"));
both("getter", withGetter, "g");
console.log(rows.join("\n"));
