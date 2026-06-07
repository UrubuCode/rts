// Cross-runtime: typed arrays expose built-in toStringTag.
const values: any[] = [new Uint8Array(1), new Int16Array(1), new Float32Array(1)];
for (const v of values) {
  console.log(Object.prototype.toString.call(v));
}
