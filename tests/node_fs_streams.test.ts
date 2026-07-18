// node:fs — ReadStream / WriteStream + createReadStream / createWriteStream.
// They extend the ambient stream Readable/Writable (so instanceof + pipe wiring
// come for free) with real file IO via the private engine.fs_* bridge.
//
// WriteStream is consumer-driven (each `.write()` is a synchronous call) and works
// end-to-end. ReadStream's construction/props/instanceof are verified here; its
// flowing data delivery shares the interim event loop's stream limitation (#207),
// so it is not asserted (the same reason node:stream tests only push synchronously).
let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import {
  createReadStream, createWriteStream, ReadStream, WriteStream,
  writeFileSync, readFileSync, existsSync, mkdirSync, rmSync,
} from "node:fs";
import { Readable, Writable } from "node:stream";

const dir = "./__fs_streams_tmp";
if (!existsSync(dir)) mkdirSync(dir);

// ---- WriteStream end-to-end (truncate) ------------------------------------
const wsPath = dir + "/out.txt";
const ws: any = createWriteStream(wsPath);
let wsFinished = false;
ws.on("finish", () => { wsFinished = true; });
ws.write("Hello, ");
ws.write("World");
ws.end("!");
const wsContent = readFileSync(wsPath, "utf8");
const wsBytes = ws.bytesWritten;
const wsIsWritable = ws instanceof Writable;
const wsIsWriteStream = ws instanceof WriteStream;
const wsPathProp = ws.path;

// ---- WriteStream append (flags: "a") --------------------------------------
const apPath = dir + "/append.txt";
writeFileSync(apPath, "base;");
const wa: any = createWriteStream(apPath, { flags: "a" });
wa.write("more");
wa.end();
const apContent = readFileSync(apPath, "utf8");

// ---- ReadStream construction + shape --------------------------------------
const rsPath = dir + "/in.txt";
writeFileSync(rsPath, "content-bytes");
const rs: any = createReadStream(rsPath);
const rsIsReadable = rs instanceof Readable;
const rsIsReadStream = rs instanceof ReadStream;
const rsPathProp = rs.path;
const rsPending = rs.pending;
const hasPipe = typeof rs.pipe === "function";
const hasOn = typeof rs.on === "function";
rs.destroy();

rmSync(dir, { recursive: true, force: true });

describe("node:fs streams", () => {
  test("createWriteStream writes the whole content", () => expect(wsContent).toBe("Hello, World!"));
  test("WriteStream tracks bytesWritten", () => expect(wsBytes).toBe(13));
  test("WriteStream emits finish on end", () => expect(wsFinished).toBe(true));
  test("WriteStream instanceof Writable", () => expect(wsIsWritable).toBe(true));
  test("WriteStream instanceof WriteStream", () => expect(wsIsWriteStream).toBe(true));
  test("WriteStream carries its path", () => expect(wsPathProp).toBe(wsPath));
  test("createWriteStream flags:a appends", () => expect(apContent).toBe("base;more"));
  test("ReadStream instanceof Readable", () => expect(rsIsReadable).toBe(true));
  test("ReadStream instanceof ReadStream", () => expect(rsIsReadStream).toBe(true));
  test("ReadStream carries its path", () => expect(rsPathProp).toBe(rsPath));
  test("ReadStream starts pending", () => expect(rsPending).toBe(true));
  test("ReadStream inherits pipe/on", () => expect(hasPipe && hasOn).toBe(true));
});
