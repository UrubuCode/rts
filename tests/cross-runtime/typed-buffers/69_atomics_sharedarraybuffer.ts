// Cross-runtime compatibility: SharedArrayBuffer and Atomics basics.
console.log("sab=" + typeof SharedArrayBuffer);
const sab = new SharedArrayBuffer(8);
const arr = new Int32Array(sab);
arr[0] = 1;
console.log("add_prev=" + Atomics.add(arr, 0, 4));
console.log("after_add=" + arr[0]);
console.log("cmp_prev=" + Atomics.compareExchange(arr, 0, 5, 9));
console.log("after_cmp=" + arr[0]);
