// Cross-runtime compatibility: BigInt typed arrays and DataView bigint ops.
const arr = new BigInt64Array(3);
arr[0] = 1n;
arr[1] = -2n;
arr[2] = 9007199254740991n;
console.log("arr=" + Array.from(arr).join(","));

const buf = new ArrayBuffer(8);
const view = new DataView(buf);
view.setBigInt64(0, -123n, false);
console.log("dv=" + view.getBigInt64(0, false).toString());
