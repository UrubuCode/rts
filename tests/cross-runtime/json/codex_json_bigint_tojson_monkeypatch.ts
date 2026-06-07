// Cross-runtime: JSON.stringify BigInt works only when toJSON is provided.
(BigInt.prototype as any).toJSON = function () {
  return this.toString() + "n";
};

console.log(JSON.stringify({ x: 5n, arr: [1n, 2n] }));
delete (BigInt.prototype as any).toJSON;
try {
  JSON.stringify(1n);
} catch (e: any) {
  console.log(e.constructor.name);
}
