// node:fs — Utf8Stream: the append-only fixed-encoding file writer. Backed by
// real file IO (engine.fs_*); writes buffer to minLength then flush, or on
// flush()/flushSync()/end(). The work is synchronous (interim event loop).
let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import { Utf8Stream, readFileSync, writeFileSync, existsSync, mkdirSync, rmSync } from "node:fs";

const dir = "./__fs_u8_tmp";
if (!existsSync(dir)) mkdirSync(dir);

// ---- truncate mode (append:false), minLength 0 flushes each write ----------
const tPath = dir + "/t.log";
writeFileSync(tPath, "OLD-DATA-SHOULD-BE-GONE");
const st: any = new Utf8Stream({ file: tPath, append: false, minLength: 0 });
let written = 0;
st.on("write", (n: any) => { written += n; });
st.write("alpha\n");
st.write("beta\n");
st.end();
const tContent = readFileSync(tPath, "utf8");

// ---- append mode (default) --------------------------------------------------
const aPath = dir + "/a.log";
writeFileSync(aPath, "head;");
const sa: any = new Utf8Stream({ file: aPath, minLength: 0 });
sa.write("tail");
sa.end();
const aContent = readFileSync(aPath, "utf8");

// ---- buffering: minLength holds writes until flush --------------------------
const bPath = dir + "/b.log";
writeFileSync(bPath, "");
const sb: any = new Utf8Stream({ file: bPath, minLength: 1000 });
sb.write("xx");
sb.write("yy");
const bBeforeFlush = readFileSync(bPath, "utf8");
sb.flushSync();
const bAfterFlush = readFileSync(bPath, "utf8");
const bAppend = sb.append;
const bContentMode = sb.contentMode;
sb.end();

// ---- maxLength drops an over-cap write --------------------------------------
const cPath = dir + "/c.log";
writeFileSync(cPath, "");
const sc: any = new Utf8Stream({ file: cPath, minLength: 1000, maxLength: 5 });
let dropped = false;
sc.on("drop", () => { dropped = true; });
sc.write("this-is-way-too-long");
sc.flushSync();
const cContent = readFileSync(cPath, "utf8");
sc.end();

rmSync(dir, { recursive: true, force: true });

describe("node:fs Utf8Stream", () => {
  test("truncate mode overwrites and writes all lines", () => expect(tContent).toBe("alpha\nbeta\n"));
  test("write emits byte counts", () => expect(written).toBe(11));
  test("append mode preserves existing content", () => expect(aContent).toBe("head;tail"));
  test("minLength buffers until flush", () => expect(bBeforeFlush).toBe(""));
  test("flushSync writes the buffer", () => expect(bAfterFlush).toBe("xxyy"));
  test("append defaults true", () => expect(bAppend).toBe(true));
  test("contentMode defaults utf8", () => expect(bContentMode).toBe("utf8"));
  test("maxLength drops an over-cap write", () => expect(dropped).toBe(true));
  test("dropped write is not persisted", () => expect(cContent).toBe(""));
});
