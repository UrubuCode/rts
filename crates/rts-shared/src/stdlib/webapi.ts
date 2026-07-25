// Faithful TypeScript web-platform value classes — Request, Response — the
// REAL stdlib for the new engine, written in `.ts` instead of hardcoded in
// codegen (same pattern as map_set.ts).
//
// `Headers`/`Blob`/`File`/`FormData` are NOT here (DRAIN_MOTOR §11, owner
// 2026-07-24): they moved to Rust `#[rtse::class]` (`rts-std/src/globals/
// headers/mod.rs`, `blob/{blob,file}.rs`, `form_data/mod.rs`, wired as
// Registry global classes) — the loader/engine needs a real Rust impl, `.ts`
// was only an interim. They're still ordinary ambient global classes
// (resolved data-driven via the Registry, identical to `URL`/
// `URLSearchParams`), so `Request`/`Response` below construct `new
// Headers(...)` exactly as before — nothing else in this file changes.
//
// These are pure VALUE HOLDERS (no I/O): `Request`/`Response` wrap a string
// body + a `Headers`. `text()` returns the string directly — `await x` passes
// a non-Promise through, so `await response.text()` behaves like the spec's
// resolved Promise.
//
// `__utf8_decode`/`__utf8_encode` below are kept — `streams.ts` and the
// `node:stream`/`node:fs` preludes still use them (they predate `Blob`, which
// used to reuse them too).

// Encode a JS string into UTF-8 bytes (a plain number[] — the byte-source
// counterpart of `__utf8_decode`; surrogate pairs → 4 bytes).
function __utf8_encode(s: string): number[] {
  const out: number[] = [];
  for (let i = 0; i < s.length; i++) {
    let cp = s.charCodeAt(i);
    if (cp >= 0xd800 && cp < 0xdc00 && i + 1 < s.length) {
      const lo = s.charCodeAt(i + 1);
      if (lo >= 0xdc00 && lo < 0xe000) {
        cp = 0x10000 + ((cp - 0xd800) << 10) + (lo - 0xdc00);
        i++;
      }
    }
    if (cp < 0x80) {
      out.push(cp);
    } else if (cp < 0x800) {
      out.push(0xc0 | (cp >> 6));
      out.push(0x80 | (cp & 0x3f));
    } else if (cp < 0x10000) {
      out.push(0xe0 | (cp >> 12));
      out.push(0x80 | ((cp >> 6) & 0x3f));
      out.push(0x80 | (cp & 0x3f));
    } else {
      out.push(0xf0 | (cp >> 18));
      out.push(0x80 | ((cp >> 12) & 0x3f));
      out.push(0x80 | ((cp >> 6) & 0x3f));
      out.push(0x80 | (cp & 0x3f));
    }
  }
  return out;
}

// Decode a UTF-8 byte source (Uint8Array-like: `.length` + numeric indexing)
// into a JS string.
function __utf8_decode(bytes: any): string {
  let out = "";
  let i = 0;
  const len = bytes.length;
  while (i < len) {
    const b0 = bytes[i];
    let cp = 0;
    let extra = 0;
    if (b0 < 0x80) { cp = b0; extra = 0; }
    else if (b0 < 0xe0) { cp = b0 & 0x1f; extra = 1; }
    else if (b0 < 0xf0) { cp = b0 & 0x0f; extra = 2; }
    else { cp = b0 & 0x07; extra = 3; }
    i++;
    for (let j = 0; j < extra; j++) {
      if (i < len) { cp = (cp << 6) | (bytes[i] & 0x3f); i++; }
    }
    out += String.fromCodePoint(cp);
  }
  return out;
}

// `FormData`/`Blob`/`File` used to live here as pure `.ts` value holders.
// DRAIN_MOTOR §11 (owner 2026-07-24): reimplemented as `#[rtse::class]` Rust
// (`rts-std/src/globals/form_data/mod.rs`, `rts-std/src/globals/blob/{blob,file}.rs`),
// wired as ordinary Registry global classes (identical pattern to `Headers`,
// which left this file the same way) — removed here now that the Rust impl
// is at parity. `__utf8_decode`/`__utf8_encode`/`__utf8_len_str` above stay:
// `streams.ts` and the `node:stream`/`node:fs` preludes still use them.

class Response {
  #body: string = "";
  headers: Headers;
  status: number = 200;
  statusText: string = "";
  ok: boolean = true;
  constructor(body: any = null, init: any = undefined) {
    if (body !== null && body !== undefined) { this.#body = "" + body; }
    let h: any = undefined;
    if (init !== undefined && init !== null) {
      const s = init.status;
      if (s !== undefined) { this.status = s; }
      const st = init.statusText;
      if (st !== undefined) { this.statusText = "" + st; }
      h = init.headers;
    }
    this.headers = new Headers(h);
    this.ok = this.status >= 200 && this.status < 300;
  }
  text(): string { return this.#body; }
  json(): any { return JSON.parse(this.#body); }
}

class Request {
  url: string;
  method: string = "GET";
  headers: Headers;
  #body: string = "";
  constructor(url: any, init: any = undefined) {
    this.url = "" + url;
    let h: any = undefined;
    if (init !== undefined && init !== null) {
      const m = init.method;
      if (m !== undefined) { this.method = "" + m; }
      const b = init.body;
      if (b !== undefined && b !== null) { this.#body = "" + b; }
      h = init.headers;
    }
    this.headers = new Headers(h);
  }
  text(): string { return this.#body; }
  json(): any { return JSON.parse(this.#body); }
}
