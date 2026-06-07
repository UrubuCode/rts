// Cross-runtime: NaN survives DataView roundtrip without depending on payload.
const buf = new ArrayBuffer(8);
const dv = new DataView(buf);
dv.setFloat64(0, NaN, false);
const bytes = Array.from(new Uint8Array(buf)).map(x => x.toString(16).padStart(2, "0")).join(" ");
console.log(Number.isNaN(dv.getFloat64(0, false)));
console.log(bytes.length > 0);
