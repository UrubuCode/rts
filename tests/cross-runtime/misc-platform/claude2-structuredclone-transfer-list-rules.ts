// Cross-runtime: the RULES around structuredClone's transfer list — what may be
// transferred, what happens to a buffer that is both transferred and reachable
// as a value, and which mistakes are a DataCloneError rather than a TypeError.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name + "/" + e.name;
  }
};

// The ordinary case: the buffer moves, the source is detached, the bytes survive.
console.log("basic=" + t(function () {
  const buf = new ArrayBuffer(4);
  new Uint8Array(buf).set([1, 2, 3, 4]);
  const out: any = structuredClone({ buf }, { transfer: [buf] });
  return out.buf.byteLength + "/" + Array.from(new Uint8Array(out.buf)).join(",") + "/" + buf.detached + "/" + buf.byteLength;
}));

// Listing the same buffer twice is a DataCloneError, and nothing is cloned.
console.log("listed_twice=" + t(function () {
  const buf = new ArrayBuffer(4);
  return structuredClone({ buf }, { transfer: [buf, buf] });
}));
console.log("listed_twice_leaves_source=" + t(function () {
  const buf = new ArrayBuffer(4);
  try {
    structuredClone({ buf }, { transfer: [buf, buf] });
  } catch (e: any) {
    return e.name + "/detached:" + buf.detached;
  }
  return "no-throw";
}));

// A buffer in the list but not in the graph is still detached.
console.log("not_in_graph=" + t(function () {
  const buf = new ArrayBuffer(4);
  const out: any = structuredClone({ x: 1 }, { transfer: [buf] });
  return out.x + "/" + buf.detached + "/" + ("buf" in out);
}));

// Reachable twice AND transferred: both references answer the SAME clone.
console.log("value_and_transfer=" + t(function () {
  const buf = new ArrayBuffer(4);
  new Uint8Array(buf)[0] = 7;
  const out: any = structuredClone({ a: buf, b: buf }, { transfer: [buf] });
  return (out.a === out.b) + "/" + new Uint8Array(out.a)[0] + "/" + buf.detached;
}));

// A VIEW is not transferable, only the buffer under it.
console.log("view_in_list=" + t(function () {
  const view = new Uint8Array(4);
  return structuredClone({ view }, { transfer: [view as any] });
}));
console.log("buffer_of_view=" + t(function () {
  const view = new Uint8Array([1, 2, 3, 4]);
  const out: any = structuredClone({ view }, { transfer: [view.buffer] });
  return Array.from(out.view).join(",") + "/" + view.length + "/" + view.buffer.detached;
}));

// Everything else in the list is refused.
console.log("plain_object=" + t(function () { return structuredClone({ o: 1 }, { transfer: [{} as any] }); }));
console.log("array=" + t(function () { return structuredClone({ o: 1 }, { transfer: [[1, 2] as any] }); }));
console.log("function=" + t(function () { return structuredClone({ o: 1 }, { transfer: [(function () { return; }) as any] }); }));
console.log("map=" + t(function () { return structuredClone({ o: 1 }, { transfer: [new Map() as any] }); }));
console.log("already_detached=" + t(function () {
  const buf = new ArrayBuffer(4);
  buf.transfer();
  return structuredClone({ buf }, { transfer: [buf] });
}));

// The list itself: empty, absent, and not an array at all.
console.log("empty_list=" + t(function () {
  const buf = new ArrayBuffer(2);
  const out: any = structuredClone({ buf }, { transfer: [] });
  return out.buf.byteLength + "/" + buf.detached + "/" + (out.buf === buf);
}));
console.log("no_options=" + t(function () {
  const buf = new ArrayBuffer(2);
  const out: any = structuredClone({ buf });
  return out.buf.byteLength + "/" + buf.detached;
}));
console.log("empty_options=" + t(function () { return structuredClone({ v: 1 }, {} as any).v; }));
console.log("undefined_transfer=" + t(function () { return structuredClone({ v: 1 }, { transfer: undefined } as any).v; }));

// Two different buffers move independently.
console.log("two_buffers=" + t(function () {
  const a = new ArrayBuffer(2);
  const b = new ArrayBuffer(4);
  const out: any = structuredClone({ a, b }, { transfer: [a] });
  return out.a.byteLength + "," + out.b.byteLength + "/" + a.detached + "," + b.detached;
}));
console.log("resizable_transferred=" + t(function () {
  const buf = new ArrayBuffer(4, { maxByteLength: 8 });
  const out: any = structuredClone({ buf }, { transfer: [buf] });
  return out.buf.resizable + "/" + out.buf.maxByteLength + "/" + buf.detached;
}));
console.log("clone_without_transfer_copies=" + t(function () {
  const buf = new ArrayBuffer(2);
  new Uint8Array(buf)[0] = 3;
  const out: any = structuredClone({ buf });
  new Uint8Array(out.buf)[0] = 9;
  return new Uint8Array(buf)[0] + "/" + new Uint8Array(out.buf)[0];
}));
console.log("error_is_domexception=" + t(function () {
  const buf = new ArrayBuffer(2);
  try {
    structuredClone({ buf }, { transfer: [{} as any] });
    return "no-throw";
  } catch (e: any) {
    return (e instanceof DOMException) + "/" + (e instanceof Error) + "/" + e.name + "/" + e.code;
  }
}));
