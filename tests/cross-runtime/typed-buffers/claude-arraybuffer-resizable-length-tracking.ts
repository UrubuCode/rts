// Cross-runtime: a resizable ArrayBuffer ({maxByteLength}) and the views over
// it. A view built with no explicit length TRACKS the buffer, a view with one
// does not — and shrinking below a fixed view puts it out of bounds.

const rb = new ArrayBuffer(4, { maxByteLength: 16 });
console.log("resizable=" + rb.resizable + " max=" + rb.maxByteLength + " len=" + rb.byteLength);
console.log("plain_resizable=" + new ArrayBuffer(4).resizable + " max=" + new ArrayBuffer(4).maxByteLength);

const tracking = new Uint8Array(rb);
const fixed = new Uint8Array(rb, 0, 4);
const trackingFromOffset = new Uint8Array(rb, 2);
tracking.set([1, 2, 3, 4]);
console.log("start=" + tracking.length + "," + fixed.length + "," + trackingFromOffset.length);

rb.resize(8);
console.log("grown_buffer=" + rb.byteLength);
console.log("grown_tracking=" + tracking.length + " bytes=" + Array.from(tracking).join(","));
console.log("grown_fixed=" + fixed.length + " bytes=" + Array.from(fixed).join(","));
console.log("grown_offset=" + trackingFromOffset.length + " bytes=" + Array.from(trackingFromOffset).join(","));

// Growing zero-fills the new region; the old bytes are kept.
tracking[7] = 9;
rb.resize(4);
console.log("shrunk_buffer=" + rb.byteLength);
console.log("shrunk_tracking=" + tracking.length + " bytes=" + Array.from(tracking).join(","));
rb.resize(8);
console.log("regrown_zeroed=" + Array.from(tracking).join(","));

// A fixed-length view whose window no longer fits reports length 0 and its
// element accesses answer undefined instead of throwing.
const far = new Uint8Array(rb, 4, 4);
console.log("far_before=" + far.length + " read=" + String(far[0]));
rb.resize(4);
console.log("far_after=" + far.length + " read=" + String(far[0]));
console.log("far_byteoffset=" + far.byteOffset + " bytelength=" + far.byteLength);
try {
  far.set([1]);
  console.log("far_set=no-throw");
} catch (e: any) {
  console.log("far_set=" + e.constructor.name);
}
try {
  console.log("far_slice=" + far.slice(0).length);
} catch (e: any) {
  console.log("far_slice=" + e.constructor.name);
}
rb.resize(8);
console.log("far_restored=" + far.length + " read=" + String(far[0]));

// A DataView with no length tracks too; one with a length does not.
const dvTrack = new DataView(rb);
const dvFixed = new DataView(rb, 0, 4);
console.log("dv_track=" + dvTrack.byteLength + " dv_fixed=" + dvFixed.byteLength);
rb.resize(6);
console.log("dv_track_after=" + dvTrack.byteLength + " dv_fixed_after=" + dvFixed.byteLength);

// Resizing to 0 and back.
rb.resize(0);
console.log("zero=" + rb.byteLength + " tracking=" + tracking.length);
rb.resize(2);
console.log("back=" + rb.byteLength + " tracking=" + tracking.length + " bytes=" + Array.from(tracking).join(","));

// resize() refuses to pass maxByteLength or go negative.
try {
  rb.resize(17);
  console.log("over_max=no-throw");
} catch (e: any) {
  console.log("over_max=" + e.constructor.name);
}
try {
  rb.resize(-1);
  console.log("negative=no-throw");
} catch (e: any) {
  console.log("negative=" + e.constructor.name);
}
console.log("after_failed_resize=" + rb.byteLength);

// A non-resizable buffer has no resize.
try {
  (new ArrayBuffer(4) as any).resize(8);
  console.log("plain_resize=no-throw");
} catch (e: any) {
  console.log("plain_resize=" + e.constructor.name);
}

// maxByteLength below the initial length, and a huge request.
try {
  new ArrayBuffer(8, { maxByteLength: 4 });
  console.log("max_below=no-throw");
} catch (e: any) {
  console.log("max_below=" + e.constructor.name);
}
console.log("max_equal=" + new ArrayBuffer(4, { maxByteLength: 4 }).resizable);
console.log("max_zero=" + new ArrayBuffer(0, { maxByteLength: 0 }).resizable);

// slice() of a resizable buffer answers a plain one.
const sliced = rb.slice(0, 2);
console.log("slice_resizable=" + sliced.resizable + " len=" + sliced.byteLength);
