// Cross-runtime: TypedArray.set copies overlapping ranges as if through a temporary list.
const forward = new Uint8Array([1, 2, 3, 4, 5, 6]);
forward.set(forward.subarray(0, 4), 2);
const backward = new Uint8Array([1, 2, 3, 4, 5, 6]);
backward.set(backward.subarray(2), 0);
console.log(forward.join(","));
console.log(backward.join(","));

