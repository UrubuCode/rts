// Cross-runtime: subarray() is a VIEW over the same buffer and slice() is a
// copy. The pair is pinned through byteOffset, aliasing, negative and
// out-of-range arguments, and what each one does across element widths.

const base = new Uint8Array([1, 2, 3, 4, 5, 6]);
const view = base.subarray(1, 4);
const copy = base.slice(1, 4);

console.log("view=" + Array.from(view).join(",") + " len=" + view.length);
console.log("copy=" + Array.from(copy).join(",") + " len=" + copy.length);
console.log("view_shares=" + (view.buffer === base.buffer));
console.log("copy_shares=" + (copy.buffer === base.buffer));
console.log("view_offset=" + view.byteOffset + " copy_offset=" + copy.byteOffset);
console.log("view_bytelen=" + view.byteLength + " copy_bytelen=" + copy.byteLength);
console.log("copy_buflen=" + copy.buffer.byteLength);

view[0] = 99;
copy[0] = 88;
console.log("base_after=" + Array.from(base).join(","));
console.log("view_after=" + Array.from(view).join(","));
console.log("copy_after=" + Array.from(copy).join(","));

// A view of a view composes offsets rather than nesting buffers.
const inner = view.subarray(1);
console.log("inner=" + Array.from(inner).join(",") + " offset=" + inner.byteOffset);
console.log("inner_shares=" + (inner.buffer === base.buffer));

// Negative, missing and reversed bounds.
const src = new Uint8Array([1, 2, 3, 4, 5]);
console.log("sub_neg=" + Array.from(src.subarray(-2)).join(","));
console.log("sub_negneg=" + Array.from(src.subarray(-4, -1)).join(","));
console.log("sub_none=" + Array.from(src.subarray()).join(","));
console.log("sub_reversed=" + src.subarray(4, 2).length + " offset=" + src.subarray(4, 2).byteOffset);
console.log("sub_past=" + Array.from(src.subarray(2, 99)).join(","));
console.log("sub_far=" + src.subarray(99).length + " offset=" + src.subarray(99).byteOffset);
console.log("slice_neg=" + Array.from(src.slice(-2)).join(","));
console.log("slice_reversed=" + src.slice(4, 2).length);
console.log("slice_past=" + Array.from(src.slice(2, 99)).join(","));
console.log("slice_undef=" + Array.from(src.slice(1, undefined)).join(","));

// Both keep the element kind, and neither is an Array.
console.log("sub_ctor=" + src.subarray(0, 1).constructor.name);
console.log("slice_ctor=" + src.slice(0, 1).constructor.name);
console.log("sub_isarray=" + Array.isArray(src.subarray(0, 1)));

// Wider elements: the offset is in BYTES while the arguments are in elements.
const wide = new Int32Array([10, 20, 30, 40]);
const wsub = wide.subarray(1, 3);
console.log("wide_sub=" + Array.from(wsub).join(",") + " byteOffset=" + wsub.byteOffset + " byteLength=" + wsub.byteLength);
const wcopy = wide.slice(1, 3);
console.log("wide_copy=" + Array.from(wcopy).join(",") + " byteOffset=" + wcopy.byteOffset + " buflen=" + wcopy.buffer.byteLength);

// A second view of a different width over the shared buffer sees the bytes.
const shared = new ArrayBuffer(8);
const asBytes = new Uint8Array(shared);
const asWords = new Uint32Array(shared);
asWords[0] = 0x01020304;
console.log("aliased=" + Array.from(asBytes.subarray(0, 4)).join(","));
asBytes.subarray(4).set([1, 0, 0, 0]);
console.log("aliased_back=" + asWords[1]);

// slice on a view copies only the view's window.
const window8 = asBytes.subarray(4, 8);
console.log("window_slice=" + Array.from(window8.slice(0)).join(",") + " buflen=" + window8.slice(0).buffer.byteLength);

// An empty view still points at its buffer.
const empty = base.subarray(3, 3);
console.log("empty_shares=" + (empty.buffer === base.buffer) + " len=" + empty.length + " offset=" + empty.byteOffset);
