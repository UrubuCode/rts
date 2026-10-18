// Cross-runtime: the typed-array constructor's refusals — a bad length, a
// misaligned or out-of-range buffer window, a non-callable iterator — with the
// error class and message, plus the accepted forms beside them.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return e.constructor.name + ": " + e.message;
  }
};

console.log(t(function () { return new Uint8Array(-1); }));
console.log(t(function () { return new Int32Array(Infinity); }));
console.log(t(function () { return new Int32Array(-Infinity); }));
console.log(t(function () { return new Float64Array(NaN).length; }));
console.log(t(function () { return new Uint8Array(3n as any); }));
console.log(t(function () { return new Uint8Array(true as any).length; }));
console.log(t(function () { return new Uint8Array(-0.5).length; }));

console.log(t(function () { return new Uint32Array(new ArrayBuffer(8), 2); }));
console.log(t(function () { return new Float64Array(new ArrayBuffer(16), 4); }));
console.log(t(function () { return new Uint32Array(new ArrayBuffer(6)); }));
console.log(t(function () { return new Uint8Array(new ArrayBuffer(4), 5); }));
console.log(t(function () { return new Uint8Array(new ArrayBuffer(4), 2, 3); }));
console.log(t(function () { return new Uint8Array(new ArrayBuffer(4), -1); }));
console.log(t(function () { return new Uint8Array(new ArrayBuffer(4), 1, -1); }));
console.log(t(function () { return new Uint8Array(new ArrayBuffer(4), 0, Infinity); }));
console.log(t(function () { return new Uint16Array(new ArrayBuffer(8), 2, 4); }));

console.log(t(function () { return new Uint8Array(new ArrayBuffer(4), 4).length; }));
console.log(t(function () { return new Uint8Array(new ArrayBuffer(4), 1.9).byteOffset; }));
console.log(t(function () { return new Uint8Array(new ArrayBuffer(4), "2").length; }));
console.log(t(function () { return new Uint32Array(new ArrayBuffer(8), 4, 1).length; }));
console.log(t(function () { return new Uint32Array(new ArrayBuffer(8), 0, undefined).length; }));
console.log(t(function () { return new Uint8Array(new ArrayBuffer(4), NaN, 2).length; }));

function sourceWith(iter: any): any {
  const src: any = { length: 1, 0: 5 };
  src[Symbol.iterator] = iter;
  return src;
}
console.log(t(function () { return new Uint8Array(sourceWith(7)); }));
console.log(t(function () { return new Uint8Array(sourceWith({})); }));
console.log(t(function () { return new Uint8Array(sourceWith(null)); }));
console.log(t(function () { return new Uint8Array(sourceWith(undefined)); }));
console.log(t(function () {
  return new Uint8Array(sourceWith(function* () { yield 1; yield 300; }));
}));
console.log(t(function () { return new Uint8Array(new Set([1, 2, 3])); }));
console.log(t(function () { return new Int16Array(new Map([[1, 2]]) as any); }));

console.log(t(function () { return new Uint8Array(3).length; }));
console.log(t(function () { return new Int32Array([1, 2]).join(","); }));
console.log(t(function () { return new Uint8Array(new ArrayBuffer(4), 0, 2).length; }));
console.log(t(function () { return new Float64Array(new Uint8Array([1, 2])).join(","); }));
console.log(t(function () { return new Uint8Array({ length: 2, 0: 5, 1: 6 } as any).join(","); }));
