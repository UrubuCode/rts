// Cross-runtime: how TextDecoder answers malformed UTF-8 — how MANY U+FFFD a
// bad sequence collapses to when lossy, that fatal:true throws a TypeError
// instead, and that stream:true holds an incomplete tail across calls.

const cps = function (s: string): string {
  const out: string[] = [];
  for (const ch of s) out.push((ch.codePointAt(0) as number).toString(16));
  return out.join(",");
};

const lossy = new TextDecoder();
const fatal = new TextDecoder("utf-8", { fatal: true });
console.log("lossy_flags=" + lossy.encoding + " fatal=" + lossy.fatal + " ignoreBOM=" + lossy.ignoreBOM);
console.log("fatal_flags=" + fatal.encoding + " fatal=" + fatal.fatal + " ignoreBOM=" + fatal.ignoreBOM);
console.log("tag=" + Object.prototype.toString.call(lossy));

const cases: Array<[string, number[]]> = [
  ["overlong_2", [0xc0, 0x80]],
  ["overlong_3", [0xe0, 0x80, 0x80]],
  ["truncated_3", [0xe2, 0x82]],
  ["truncated_4", [0xf0, 0x9f, 0x98]],
  ["surrogate_ed", [0xed, 0xa0, 0x80]],
  ["lone_continuation", [0x80]],
  ["ff_byte", [0xff]],
  ["fe_byte", [0xfe]],
  ["above_max", [0xf5, 0x80, 0x80, 0x80]],
  ["c1_start", [0xc1, 0xbf]],
  ["bad_then_ascii", [0xff, 0x41]],
  ["truncated_then_ascii", [0xe2, 0x41]],
  ["valid_after_bad", [0x80, 0xc3, 0xa9]],
];
for (const c of cases) {
  const bytes = new Uint8Array(c[1]);
  const text = lossy.decode(bytes);
  let outcome = "no-throw";
  try {
    fatal.decode(bytes);
  } catch (e: any) {
    outcome = e.constructor.name;
  }
  console.log(c[0] + " lossy=" + cps(text) + " len=" + text.length + " fatal=" + outcome);
}

// Valid sequences of every width still decode after the fatal decoder threw.
console.log("still_usable=" + cps(fatal.decode(new Uint8Array([0x41, 0xc3, 0xa9, 0xe2, 0x82, 0xac, 0xf0, 0x9f, 0x98, 0x80]))));

// The BOM is consumed by default and kept when ignoreBOM is set.
const bom = new Uint8Array([0xef, 0xbb, 0xbf, 0x41]);
console.log("bom_default=" + cps(lossy.decode(bom)));
console.log("bom_ignored=" + cps(new TextDecoder("utf-8", { ignoreBOM: true }).decode(bom)));
console.log("bom_only=" + lossy.decode(new Uint8Array([0xef, 0xbb, 0xbf])).length);
console.log("bom_mid=" + cps(lossy.decode(new Uint8Array([0x41, 0xef, 0xbb, 0xbf]))));

// Streaming: an incomplete tail is remembered, not replaced.
const enc = new TextEncoder();
const whole = enc.encode("é€\u{1F600}!");
const streamer = new TextDecoder();
let assembled = "";
for (let i = 0; i < whole.length; i++) {
  assembled += streamer.decode(whole.subarray(i, i + 1), { stream: true });
}
assembled += streamer.decode();
console.log("stream_byte_by_byte=" + (assembled === "é€\u{1F600}!") + " len=" + assembled.length);

const halves = new TextDecoder();
console.log("stream_first_half=" + cps(halves.decode(whole.subarray(0, 4), { stream: true })));
console.log("stream_second_half=" + cps(halves.decode(whole.subarray(4), { stream: true })));
console.log("stream_flush=" + halves.decode().length);

// Without stream:true the same split produces replacements at the seam.
const nonStreaming = new TextDecoder();
console.log("nostream_first=" + cps(nonStreaming.decode(whole.subarray(0, 4))));
console.log("nostream_second=" + cps(nonStreaming.decode(whole.subarray(4))));

// A held tail that turns out to be truncated is reported when the stream ends.
const flushFatal = new TextDecoder("utf-8", { fatal: true });
console.log("held_tail=" + flushFatal.decode(enc.encode("€").subarray(0, 2), { stream: true }).length);
try {
  flushFatal.decode();
  console.log("flush_incomplete=no-throw");
} catch (e: any) {
  console.log("flush_incomplete=" + e.constructor.name);
}
const flushLossy = new TextDecoder();
console.log("flush_lossy_held=" + flushLossy.decode(enc.encode("€").subarray(0, 2), { stream: true }).length);
console.log("flush_lossy=" + cps(flushLossy.decode()));

// decode() with no argument, an empty view, and an ArrayBuffer source.
console.log("no_arg=" + JSON.stringify(new TextDecoder().decode()));
console.log("empty=" + JSON.stringify(lossy.decode(new Uint8Array(0))));
console.log("from_buffer=" + cps(lossy.decode(enc.encode("A").buffer)));
console.log("from_dataview=" + cps(lossy.decode(new DataView(enc.encode("A").buffer))));
console.log("from_subarray=" + cps(lossy.decode(enc.encode("AB").subarray(1))));
