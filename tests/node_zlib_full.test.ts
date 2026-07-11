// node:zlib — synchronous compression surface (round-trips + constants).
import { describe, test, expect } from "rts:test";
import {
    deflateSync,
    inflateSync,
    deflateRawSync,
    inflateRawSync,
    gzipSync,
    gunzipSync,
    unzipSync,
    brotliCompressSync,
    brotliDecompressSync,
    constants,
} from "node:zlib";

// Input: a repetitive string (compresses well) as a Uint8Array of its bytes.
const text = "hello hello hello world world compression test 12345 12345";
const bytes: number[] = [];
for (let i = 0; i < text.length; i = i + 1) bytes.push(text.charCodeAt(i));
const input = new Uint8Array(bytes);
const inputLen = input.length;

function decodeAscii(buf: Uint8Array): string {
    let s = "";
    for (let i = 0; i < buf.length; i = i + 1) s = s + String.fromCharCode(buf[i]);
    return s;
}

// --- deflate / inflate round-trip -------------------------------------------
const def = deflateSync(input);
const defLen = def.length;
const infl = inflateSync(def);
const inflRoundtrip = decodeAscii(infl) === text;
const compressedSmaller = defLen < inputLen; // repetitive → smaller

// --- deflateRaw / inflateRaw ------------------------------------------------
const rawRoundtrip = decodeAscii(inflateRawSync(deflateRawSync(input))) === text;

// --- gzip / gunzip ----------------------------------------------------------
const gz = gzipSync(input);
const gzMagic = gz[0] === 0x1f && gz[1] === 0x8b; // gzip header
const gunzipRoundtrip = decodeAscii(gunzipSync(gz)) === text;

// --- unzip auto-detect (gzip) -----------------------------------------------
const unzipGzRoundtrip = decodeAscii(unzipSync(gz)) === text;
// unzip auto-detect (zlib/deflate)
const unzipDeflateRoundtrip = decodeAscii(unzipSync(def)) === text;

// --- brotli -----------------------------------------------------------------
const br = brotliCompressSync(input);
const brRoundtrip = decodeAscii(brotliDecompressSync(br)) === text;

// --- constants --------------------------------------------------------------
const cFinish = constants.Z_FINISH;
const cBestComp = constants.Z_BEST_COMPRESSION;
const cGzip = constants.GZIP;
const cBrotliQ = constants.BROTLI_MAX_QUALITY;

describe("node:zlib sync surface", () => {
    test("deflate/inflate round-trip", () => expect(inflRoundtrip).toBe(true));
    test("deflate compresses repetitive input", () => expect(compressedSmaller).toBe(true));
    test("deflateRaw/inflateRaw round-trip", () => expect(rawRoundtrip).toBe(true));
    test("gzip header magic", () => expect(gzMagic).toBe(true));
    test("gzip/gunzip round-trip", () => expect(gunzipRoundtrip).toBe(true));
    test("unzip auto-detect gzip", () => expect(unzipGzRoundtrip).toBe(true));
    test("unzip auto-detect deflate", () => expect(unzipDeflateRoundtrip).toBe(true));
    test("brotli round-trip", () => expect(brRoundtrip).toBe(true));
    test("constants Z_FINISH", () => expect(cFinish).toBe(4));
    test("constants Z_BEST_COMPRESSION", () => expect(cBestComp).toBe(9));
    test("constants GZIP", () => expect(cGzip).toBe(3));
    test("constants BROTLI_MAX_QUALITY", () => expect(cBrotliQ).toBe(11));
});
