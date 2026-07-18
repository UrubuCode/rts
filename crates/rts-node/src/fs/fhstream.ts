// node:fs/promises — the `open()` wrapper. `fs.promises.open` returns a native
// FileHandle (its read/write/close/stat/truncate/sync/chmod methods are native);
// this wrapper augments that handle with the STREAM methods, which the native
// side cannot construct because they return `.ts` stream instances. Each closes
// over the opened `path`, so a `ReadStream`/`WriteStream`/`ReadableStream` over
// the same file is one call away. The native handle is opened synchronously via
// the private `engine.fs_open_handle` bridge (throws → `open` rejects).

// A minimal `readline.Interface` over an already-read file: the line reader
// `filehandle.readLines()` returns. Iterable (`for await (const line of …)` uses
// the `*[Symbol.iterator]` via the sync-iterator fallback) and an EventEmitter
// (`on('line')`/`on('close')`); `close()`/`pause()`/`resume()` are present for
// the Interface shape. Lines are split on `\n` with a trailing `\r` stripped
// (CRLF), and a final empty line (trailing newline) dropped — Node's behavior.
class __FileHandleLineReader extends Stream {
  __lines: any;
  constructor(text: any) {
    super();
    this.__lines = __fsSplitLines(text);
  }
  *[Symbol.iterator](): any {
    const self: any = this;
    let i = 0;
    while (i < self.__lines.length) {
      const line: any = self.__lines[i];
      i = i + 1;
      self.emit("line", line);
      yield line;
    }
    self.emit("close");
  }
  close(): void { const self: any = this; self.emit("close"); }
  pause(): any { return this; }
  resume(): any { return this; }
}

function __fsSplitLines(text: any): any {
  const s: any = "" + text;
  const raw: any = s.split("\n");
  const out: any[] = [];
  for (let i = 0; i < raw.length; i++) {
    let line: any = raw[i];
    // Strip a trailing CR (CRLF line endings).
    if (line.length > 0 && line.charCodeAt(line.length - 1) === 13) {
      line = line.slice(0, line.length - 1);
    }
    // Drop a final empty element produced by a trailing newline.
    if (i === raw.length - 1 && line.length === 0) { break; }
    out.push(line);
  }
  return out;
}

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

// The `readline.Interface` line reader over the file at `path` (top-level so the
// class construction does not happen inside the fh property-method closure).
function __fsReadLinesOf(path: any): any {
  return new __FileHandleLineReader(__utf8_decode(engine.fs_read_bytes(path)));
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
  fh.readLines = function (options: any = undefined): any {
    return __fsReadLinesOf(p);
  };
  return Promise.resolve(fh);
}
