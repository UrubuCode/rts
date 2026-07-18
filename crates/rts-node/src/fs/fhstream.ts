// node:fs/promises — the `open()` wrapper. `fs.promises.open` returns a native
// FileHandle (its read/write/close/stat/truncate/sync/chmod methods are native);
// this wrapper augments that handle with the STREAM methods, which the native
// side cannot construct because they return `.ts` stream instances. Each closes
// over the opened `path`, so a `ReadStream`/`WriteStream`/`ReadableStream` over
// the same file is one call away. The native handle is opened synchronously via
// the private `engine.fs_open_handle` bridge (throws → `open` rejects).

// A WHATWG ReadableStream delivering `bytes` as one chunk then EOF. A top-level
// helper (constructing it inside the fh property-method's closure loses the
// `start` enqueue in this engine).
function __fsWebStreamOf(bytes: any): any {
  return new ReadableStream({
    start(controller: any): void {
      controller.enqueue(bytes);
      controller.close();
    },
  });
}

function __fsPromisesOpen(path: any, flags: any = "r"): any {
  const fh: any = engine.fs_open_handle(path, flags);
  const p: any = path;
  fh.createReadStream = function (options: any = undefined): any {
    return createReadStream(p, options);
  };
  fh.createWriteStream = function (options: any = undefined): any {
    return createWriteStream(p, options);
  };
  fh.readableWebStream = function (options: any = undefined): any {
    return __fsWebStreamOf(engine.fs_read_bytes(p));
  };
  return Promise.resolve(fh);
}
