// Cross-runtime: the RangeError sites the specification names — invalid array
// lengths, out-of-range numeric formatting arguments, bad radices and negative
// repeat counts — plus the boundary values that must NOT throw.
function probe(fn: () => any): string {
  try {
    const v = fn();
    return "ok:" + String(v);
  } catch (e: any) {
    return e.constructor.name;
  }
}

console.log("array-neg=" + probe(() => new Array(-1)));
console.log("array-frac=" + probe(() => new Array(1.5)));
console.log("array-huge=" + probe(() => new Array(4294967296)));
console.log("array-max-ok=" + probe(() => new Array(4294967295).length));
console.log("array-zero-ok=" + probe(() => new Array(0).length));
console.log("array-string-ok=" + probe(() => new Array("3" as any).length));
console.log("array-two-args-ok=" + probe(() => new Array(1, 2).length));

class Sizer {
  static setLength(v: any): string {
    try {
      const a: any = [];
      a.length = v;
      return "ok:" + a.length;
    } catch (e: any) {
      return e.constructor.name;
    }
  }
}
console.log("length-neg=" + Sizer.setLength(-1));
console.log("length-frac=" + Sizer.setLength(1.5));
console.log("length-nan=" + Sizer.setLength(NaN));
console.log("length-ok=" + Sizer.setLength(3));

console.log("tofixed-101=" + probe(() => (1).toFixed(101)));
console.log("tofixed-neg=" + probe(() => (1).toFixed(-1)));
console.log("tofixed-100-ok=" + probe(() => (1).toFixed(100).length));
console.log("tofixed-0-ok=" + probe(() => (1.6).toFixed(0)));

console.log("toprecision-0=" + probe(() => (1).toPrecision(0)));
console.log("toprecision-102=" + probe(() => (1).toPrecision(102)));
console.log("toprecision-1-ok=" + probe(() => (123).toPrecision(1)));
console.log("toprecision-100-ok=" + probe(() => (1).toPrecision(100).length));
console.log("toprecision-undefined-ok=" + probe(() => (1.5).toPrecision(undefined)));

console.log("toexponential-101=" + probe(() => (1).toExponential(101)));
console.log("toexponential-neg=" + probe(() => (1).toExponential(-1)));
console.log("toexponential-2-ok=" + probe(() => (1234).toExponential(2)));

console.log("tostring-37=" + probe(() => (255).toString(37)));
console.log("tostring-1=" + probe(() => (255).toString(1)));
console.log("tostring-0=" + probe(() => (255).toString(0)));
console.log("tostring-frac=" + probe(() => (255).toString(2.5)));
console.log("tostring-2-ok=" + probe(() => (255).toString(2)));
console.log("tostring-36-ok=" + probe(() => (255).toString(36)));

console.log("repeat-neg=" + probe(() => "ab".repeat(-1)));
console.log("repeat-inf=" + probe(() => "ab".repeat(Infinity)));
console.log("repeat-0-ok=" + probe(() => JSON.stringify("ab".repeat(0))));
console.log("repeat-frac-ok=" + probe(() => "ab".repeat(2.9)));

console.log("padstart-huge=" + probe(() => "a".padStart(Number.MAX_SAFE_INTEGER, "x").length));
console.log("padstart-ok=" + probe(() => "a".padStart(4, "-")));

console.log("normalize-bad=" + probe(() => "a".normalize("NFX" as any)));
console.log("normalize-ok=" + probe(() => "a".normalize("NFC")));

console.log("fromcodepoint-neg=" + probe(() => String.fromCodePoint(-1)));
console.log("fromcodepoint-frac=" + probe(() => String.fromCodePoint(1.5)));
console.log("fromcodepoint-huge=" + probe(() => String.fromCodePoint(0x110000)));
console.log("fromcodepoint-ok=" + probe(() => String.fromCodePoint(65)));

console.log("bigint-frac=" + probe(() => BigInt(1.5)));
console.log("bigint-nan=" + probe(() => BigInt(NaN)));
console.log("bigint-int-ok=" + probe(() => BigInt(3)));
console.log("bigint-asuintn-neg=" + probe(() => BigInt.asUintN(-1, 1n)));

console.log("typedarray-neg=" + probe(() => new Uint8Array(-1)));
console.log("typedarray-frac=" + probe(() => new Uint8Array(1.5).length));
console.log("typedarray-ok=" + probe(() => new Uint8Array(2).length));
console.log("arraybuffer-neg=" + probe(() => new ArrayBuffer(-1)));
console.log("dataview-offset=" + probe(() => new DataView(new ArrayBuffer(4), 8)));
console.log("dataview-ok=" + probe(() => new DataView(new ArrayBuffer(4), 2).byteLength));

console.log("date-invalid-toiso=" + probe(() => new Date(NaN).toISOString()));
console.log("date-valid-toiso-ok=" + probe(() => new Date(0).toISOString()));

// Every one of these is a RangeError, and a RangeError is an Error.
console.log("is-range-tofixed=" + isRange(() => (1).toFixed(101)));
console.log("is-range-radix=" + isRange(() => (1).toString(37)));
console.log("is-range-repeat=" + isRange(() => "a".repeat(-1)));
console.log("is-range-array=" + isRange(() => new Array(-1)));
function isRange(fn: () => any): string {
  try {
    fn();
    return "no-throw";
  } catch (e: any) {
    return String(e instanceof RangeError) + ":" + String(e instanceof Error);
  }
}
