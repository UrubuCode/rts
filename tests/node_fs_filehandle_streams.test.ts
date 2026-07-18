// node:fs/promises — FileHandle stream methods. `promises.open` returns the
// native FileHandle (read/write/close/stat/... stay native) augmented with the
// `.ts` stream factories: createReadStream / createWriteStream / readableWebStream.
let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import { open } from "node:fs/promises";
import { writeFileSync, readFileSync, existsSync, mkdirSync, rmSync } from "node:fs";


const dir = "./__fs_fhstream_tmp";
if (!existsSync(dir)) mkdirSync(dir);

// ---- FileHandle.createReadStream + pipe to a WriteStream -------------------
const srcPath = dir + "/src.txt";
writeFileSync(srcPath, "handle-read-stream-content");
const rh: any = await open(srcPath, "r");
const nativeRead: string = await rh.readFile("utf8");   // native method still works
const rs: any = rh.createReadStream();
const rsHasPipe = typeof rs.pipe === "function" && typeof rs.on === "function";
let flowLen = 0;
rs.on("data", (c: any) => { flowLen += c.length; });
await rh.close();

// ---- FileHandle.createWriteStream end-to-end ------------------------------
const dstPath = dir + "/dst.txt";
const wh: any = await open(dstPath, "w");
const ws: any = wh.createWriteStream();
const wsHasWrite = typeof ws.write === "function" && typeof ws.end === "function";
ws.write("written-through-");
ws.end("filehandle");
const dstContent = readFileSync(dstPath, "utf8");
await wh.close();

// ---- FileHandle.readableWebStream (WHATWG) --------------------------------
const webPath = dir + "/web.txt";
writeFileSync(webPath, "web-content");
const wfh: any = await open(webPath, "r");
const web: any = wfh.readableWebStream();
const hasGetReader = typeof web.getReader === "function";
const reader: any = web.getReader();
const chunk: any = await reader.read();
const webLen = chunk.done ? 0 : chunk.value.length;
await wfh.close();

// ---- FileHandle.readLines (readline.Interface) ----------------------------
const linesPath = dir + "/lines.txt";
writeFileSync(linesPath, "alpha\r\nbeta\ngamma\n");
const lfh: any = await open(linesPath, "r");
const rl: any = lfh.readLines();
const lineEvents: any[] = [];
rl.on("line", (l: any) => { lineEvents.push(l); });
const lineIter: any[] = [];
for await (const line of rl) { lineIter.push(line); }
await lfh.close();
const iterJoined = lineIter.join("|");
const eventJoined = lineEvents.join("|");

rmSync(dir, { recursive: true, force: true });

describe("node:fs FileHandle streams", () => {
  test("native readFile still works", () => expect(nativeRead).toBe("handle-read-stream-content"));
  test("createReadStream is a readable stream (pipe/on)", () => expect(rsHasPipe).toBe(true));
  test("createReadStream delivers all bytes", () => expect(flowLen).toBe(26));
  test("createWriteStream is a writable stream (write/end)", () => expect(wsHasWrite).toBe(true));
  test("createWriteStream writes end-to-end", () => expect(dstContent).toBe("written-through-filehandle"));
  test("readableWebStream exposes getReader", () => expect(hasGetReader).toBe(true));
  test("readableWebStream delivers the file bytes", () => expect(webLen).toBe(11));
  test("readLines iterates lines (CRLF + trailing-newline handled)", () => expect(iterJoined).toBe("alpha|beta|gamma"));
  test("readLines emits 'line' events", () => expect(eventJoined).toBe("alpha|beta|gamma"));
});
