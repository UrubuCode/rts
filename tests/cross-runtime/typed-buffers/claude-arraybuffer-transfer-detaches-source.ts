// Cross-runtime: ArrayBuffer.prototype.transfer moves the bytes and DETACHES
// the source. Every view over the detached buffer keeps its identity but
// reports length 0 — and which operations answer undefined vs throw is the
// point being pinned.

const src = new ArrayBuffer(4);
const view = new Uint8Array(src);
const dv = new DataView(src);
view.set([1, 2, 3, 4]);
console.log("before_detached=" + src.detached + " len=" + src.byteLength);

const moved = src.transfer();
console.log("after_detached=" + src.detached + " src_len=" + src.byteLength);
console.log("moved_len=" + moved.byteLength + " bytes=" + Array.from(new Uint8Array(moved)).join(","));
console.log("moved_detached=" + moved.detached + " same_object=" + ((moved as any) === (src as any)));

// The view is still an object of its kind; only its window is gone.
console.log("view_kind=" + view.constructor.name + " len=" + view.length + " bytelen=" + view.byteLength + " byteoffset=" + view.byteOffset);
console.log("view_buffer_is_src=" + (view.buffer === src));

// Reads answer undefined and writes are dropped. The write goes through
// Reflect.set so that the [[Set]] RESULT is what gets pinned: a bare assignment
// throws in strict code and is silent in sloppy code when [[Set]] answers
// false, which would measure the caller's mode rather than the detached view.
console.log("read=" + String(view[0]));
console.log("set_result=" + Reflect.set(view, "0", 9));
console.log("write_then_read=" + String(view[0]));
console.log("in_operator=" + ("0" in view));
console.log("keys=" + JSON.stringify(Object.keys(view)));

// Methods that walk the elements throw instead.
const attempts: string[] = ["set", "slice", "subarray", "fill", "sort", "indexOf", "join", "iterate", "at", "copyWithin", "toSorted"];
for (const name of attempts) {
  let outcome = "no-throw";
  try {
    if (name === "set") view.set([1]);
    else if (name === "slice") view.slice(0);
    else if (name === "subarray") outcome = "len:" + view.subarray(0).length;
    else if (name === "fill") view.fill(0);
    else if (name === "sort") view.sort();
    else if (name === "indexOf") outcome = "idx:" + view.indexOf(1);
    else if (name === "join") outcome = "join:" + JSON.stringify(view.join(","));
    else if (name === "iterate") outcome = "iter:" + Array.from(view).length;
    else if (name === "at") outcome = "at:" + String(view.at(0));
    else if (name === "copyWithin") view.copyWithin(0, 1);
    else if (name === "toSorted") view.toSorted();
  } catch (e: any) {
    outcome = e.constructor.name;
  }
  console.log("detached_" + name + "=" + outcome);
}

// The DataView over the same detached buffer throws on any access.
try {
  dv.getUint8(0);
  console.log("dv_read=no-throw");
} catch (e: any) {
  console.log("dv_read=" + e.constructor.name);
}
try {
  console.log("dv_bytelength=" + dv.byteLength);
} catch (e: any) {
  console.log("dv_bytelength=" + e.constructor.name);
}

// A new view cannot be built over a detached buffer.
try {
  new Uint8Array(src);
  console.log("new_view=no-throw");
} catch (e: any) {
  console.log("new_view=" + e.constructor.name);
}
try {
  src.slice(0);
  console.log("detached_buffer_slice=no-throw");
} catch (e: any) {
  console.log("detached_buffer_slice=" + e.constructor.name);
}
try {
  src.transfer();
  console.log("transfer_again=no-throw");
} catch (e: any) {
  console.log("transfer_again=" + e.constructor.name);
}

// transfer(newLength) truncates or zero-extends.
const grow = new ArrayBuffer(2);
new Uint8Array(grow).set([7, 8]);
const grown = grow.transfer(4);
console.log("grown=" + grown.byteLength + " bytes=" + Array.from(new Uint8Array(grown)).join(","));
const shrunk = grown.transfer(1);
console.log("shrunk=" + shrunk.byteLength + " bytes=" + Array.from(new Uint8Array(shrunk)).join(","));
console.log("zero=" + shrunk.transfer(0).byteLength);

// transferToFixedLength drops resizability.
const rb = new ArrayBuffer(2, { maxByteLength: 8 });
const fixedOut = rb.transferToFixedLength(4);
console.log("fixed_out=" + fixedOut.resizable + " len=" + fixedOut.byteLength + " src_detached=" + rb.detached);
const rb2 = new ArrayBuffer(2, { maxByteLength: 8 });
console.log("transfer_keeps_resizable=" + rb2.transfer().resizable);

// structuredClone with a transfer list detaches too, and the clone is the same
// bytes under a new buffer.
const cloneSrc = new ArrayBuffer(3);
new Uint8Array(cloneSrc).set([4, 5, 6]);
const clone = structuredClone(cloneSrc, { transfer: [cloneSrc] });
console.log("clone_detached_src=" + cloneSrc.detached + " clone=" + Array.from(new Uint8Array(clone)).join(","));
try {
  structuredClone(clone, { transfer: [new ArrayBuffer(1)] });
  console.log("transfer_not_in_graph=no-throw");
} catch (e: any) {
  console.log("transfer_not_in_graph=" + e.constructor.name);
}
