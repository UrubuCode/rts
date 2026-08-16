// Cross-runtime: the REFUSALS around resize and transfer. resize needs a
// resizable buffer and a length within maxByteLength, transfer takes a new
// length and decides resizability, and a detached buffer refuses both.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

// resize is only reachable on a buffer created with maxByteLength.
console.log("resize_exists=" + typeof (ArrayBuffer.prototype as any).resize + " transfer=" + typeof ArrayBuffer.prototype.transfer + " toFixed=" + typeof (ArrayBuffer.prototype as any).transferToFixedLength);
console.log("resize_non_resizable=" + t(function () { return (new ArrayBuffer(4) as any).resize(8); }));
console.log("resize_over_max=" + t(function () { return (new ArrayBuffer(4, { maxByteLength: 8 }) as any).resize(9); }));
console.log("resize_negative=" + t(function () { return (new ArrayBuffer(4, { maxByteLength: 8 }) as any).resize(-1); }));
console.log("resize_fraction=" + t(function () { const b: any = new ArrayBuffer(4, { maxByteLength: 8 }); b.resize(5.9); return b.byteLength; }));
console.log("resize_string=" + t(function () { const b: any = new ArrayBuffer(4, { maxByteLength: 8 }); b.resize("6"); return b.byteLength; }));
console.log("resize_undefined=" + t(function () { const b: any = new ArrayBuffer(4, { maxByteLength: 8 }); b.resize(undefined); return b.byteLength; }));
console.log("resize_to_zero=" + t(function () { const b: any = new ArrayBuffer(4, { maxByteLength: 8 }); b.resize(0); return b.byteLength; }));
console.log("resize_to_max=" + t(function () { const b: any = new ArrayBuffer(4, { maxByteLength: 8 }); b.resize(8); return b.byteLength; }));
console.log("resize_returns=" + t(function () { return String((new ArrayBuffer(2, { maxByteLength: 4 }) as any).resize(3)); }));

// Growing zero-fills, shrinking then growing does not bring the bytes back.
console.log("grow_zero_fills=" + t(function () {
  const b: any = new ArrayBuffer(2, { maxByteLength: 6 });
  new Uint8Array(b).set([7, 7]);
  b.resize(4);
  return Array.from(new Uint8Array(b)).join(",");
}));
console.log("shrink_then_grow=" + t(function () {
  const b: any = new ArrayBuffer(4, { maxByteLength: 8 });
  new Uint8Array(b).set([1, 2, 3, 4]);
  b.resize(2);
  b.resize(4);
  return Array.from(new Uint8Array(b)).join(",");
}));

// The constructor's own refusals.
console.log("ctor_over_max=" + t(function () { return new ArrayBuffer(8, { maxByteLength: 4 }).byteLength; }));
console.log("ctor_max_negative=" + t(function () { return new ArrayBuffer(0, { maxByteLength: -1 }); }));
console.log("ctor_max_fraction=" + t(function () { return new ArrayBuffer(0, { maxByteLength: 4.9 }).maxByteLength; }));
console.log("ctor_max_undefined=" + t(function () { const b = new ArrayBuffer(4, { maxByteLength: undefined } as any); return b.resizable + "/" + b.maxByteLength; }));
console.log("ctor_options_ignored=" + t(function () { const b = new ArrayBuffer(4, { other: 9 } as any); return b.resizable + "/" + b.maxByteLength; }));
console.log("ctor_length_negative=" + t(function () { return new ArrayBuffer(-1); }));
console.log("ctor_length_string=" + t(function () { return new ArrayBuffer("4" as any).byteLength; }));
console.log("no_new=" + t(function () { return (ArrayBuffer as any)(4); }));

// transfer: the new length decides truncation or zero-fill, and the source is
// left detached whichever way it goes.
console.log("transfer_same=" + t(function () {
  const b = new ArrayBuffer(4);
  new Uint8Array(b).set([1, 2, 3, 4]);
  const c = b.transfer();
  return c.byteLength + "/" + Array.from(new Uint8Array(c)).join(",") + "/" + b.detached;
}));
console.log("transfer_grow=" + t(function () {
  const b = new ArrayBuffer(2);
  new Uint8Array(b).set([1, 2]);
  const c = b.transfer(4);
  return c.byteLength + "/" + Array.from(new Uint8Array(c)).join(",");
}));
console.log("transfer_shrink=" + t(function () {
  const b = new ArrayBuffer(4);
  new Uint8Array(b).set([1, 2, 3, 4]);
  return Array.from(new Uint8Array(b.transfer(2))).join(",");
}));
console.log("transfer_zero=" + t(function () { return new ArrayBuffer(4).transfer(0).byteLength; }));
console.log("transfer_negative=" + t(function () { return new ArrayBuffer(4).transfer(-1); }));
console.log("transfer_fraction=" + t(function () { return new ArrayBuffer(4).transfer(2.9 as any).byteLength; }));
console.log("transfer_twice=" + t(function () { const b = new ArrayBuffer(4); b.transfer(); return b.transfer(); }));
console.log("resize_after_transfer=" + t(function () { const b: any = new ArrayBuffer(4, { maxByteLength: 8 }); b.transfer(); return b.resize(6); }));
console.log("byteLength_after_transfer=" + t(function () { const b = new ArrayBuffer(4); b.transfer(); return b.byteLength + "/" + b.detached + "/" + b.resizable + "/" + b.maxByteLength; }));

// Resizability is preserved by transfer and dropped by transferToFixedLength.
console.log("transfer_keeps_resizable=" + t(function () {
  const b = new ArrayBuffer(2, { maxByteLength: 8 });
  const c = b.transfer();
  return c.resizable + "/" + c.maxByteLength + "/" + c.byteLength;
}));
console.log("to_fixed_drops_it=" + t(function () {
  const b = new ArrayBuffer(2, { maxByteLength: 8 });
  const c = (b as any).transferToFixedLength();
  return c.resizable + "/" + c.byteLength + "/" + c.maxByteLength + "/" + b.detached;
}));
console.log("to_fixed_length_arg=" + t(function () { return (new ArrayBuffer(4) as any).transferToFixedLength(6).byteLength; }));
console.log("transfer_of_plain=" + t(function () { const c = new ArrayBuffer(4).transfer(); return c.resizable + "/" + c.maxByteLength; }));
console.log("wrong_receiver=" + t(function () { return (ArrayBuffer.prototype.transfer as any).call({}); }) + " " + t(function () { return (ArrayBuffer.prototype.transfer as any).call(new Uint8Array(4)); }));
