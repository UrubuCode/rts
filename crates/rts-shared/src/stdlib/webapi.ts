// Faithful TypeScript web-platform value classes — Headers, FormData, Blob,
// File, Request, Response — the REAL stdlib for the new engine, written in
// `.ts` instead of hardcoded in codegen (same pattern as map_set.ts).
//
// These are pure VALUE HOLDERS (no I/O): parallel private arrays keep entries
// in insertion order; `Headers` lowercases names and combines duplicates with
// ", " on read (fetch spec), iterating in sorted-name order; `Blob`/`File`
// carry string / Uint8Array-like parts measured and decoded as UTF-8 (the
// helpers below); `Request`/`Response` wrap a string body + a `Headers`.
// `text()` returns the string directly — `await x` passes a non-Promise
// through, so `await blob.text()` behaves like the spec's resolved Promise.

// UTF-8 byte length of a JS string (surrogate pairs → 4 bytes).
function __utf8_len_str(s: string): number {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c < 0x80) { n += 1; }
    else if (c < 0x800) { n += 2; }
    else if (c >= 0xd800 && c < 0xdc00 && i + 1 < s.length) { n += 4; i++; }
    else { n += 3; }
  }
  return n;
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

class Headers {
  #names: string[] = [];
  #values: string[] = [];
  // `new Headers()` / `new Headers([[k, v], …])` / `new Headers({ k: v })` /
  // `new Headers(other)` (anything with `entries()`).
  constructor(init: any = undefined) {
    if (init === undefined || init === null) { return; }
    if (Array.isArray(init)) {
      for (const pair of init) { this.append(pair[0], pair[1]); }
      return;
    }
    const ks = Object.keys(init);
    for (let i = 0; i < ks.length; i++) { this.append(ks[i], init[ks[i]]); }
  }
  append(name: any, value: any): void {
    this.#names.push(("" + name).toLowerCase());
    this.#values.push("" + value);
  }
  delete(name: any): void {
    const n = ("" + name).toLowerCase();
    const keepN: string[] = [];
    const keepV: string[] = [];
    for (let i = 0; i < this.#names.length; i++) {
      if (this.#names[i] !== n) { keepN.push(this.#names[i]); keepV.push(this.#values[i]); }
    }
    this.#names = keepN;
    this.#values = keepV;
  }
  set(name: any, value: any): void {
    this.delete(name);
    this.append(name, value);
  }
  // Combined value (duplicates joined ", "), `null` on miss (fetch spec).
  get(name: any): any {
    const n = ("" + name).toLowerCase();
    const out: string[] = [];
    for (let i = 0; i < this.#names.length; i++) {
      if (this.#names[i] === n) { out.push(this.#values[i]); }
    }
    if (out.length === 0) { return null; }
    return out.join(", ");
  }
  has(name: any): boolean {
    const n = ("" + name).toLowerCase();
    for (let i = 0; i < this.#names.length; i++) {
      if (this.#names[i] === n) return true;
    }
    return false;
  }
  getSetCookie(): string[] {
    const out: string[] = [];
    for (let i = 0; i < this.#names.length; i++) {
      if (this.#names[i] === "set-cookie") { out.push(this.#values[i]); }
    }
    return out;
  }
  // Iteration order (fetch spec): unique names sorted lexicographically,
  // duplicate values combined with ", " — EXCEPT `set-cookie`, which yields
  // one entry per value.
  keys(): string[] {
    const uniq: string[] = [];
    for (let i = 0; i < this.#names.length; i++) {
      const n = this.#names[i];
      let seen = false;
      for (let j = 0; j < uniq.length; j++) {
        if (uniq[j] === n) { seen = true; break; }
      }
      if (!seen) { uniq.push(n); }
    }
    uniq.sort();
    const out: string[] = [];
    for (let i = 0; i < uniq.length; i++) {
      if (uniq[i] === "set-cookie") {
        const sc = this.getSetCookie();
        for (let j = 0; j < sc.length; j++) { out.push("set-cookie"); }
      } else {
        out.push(uniq[i]);
      }
    }
    return out;
  }
  entries(): [string, string][] {
    const out: [string, string][] = [];
    const uniq: string[] = [];
    for (let i = 0; i < this.#names.length; i++) {
      const n = this.#names[i];
      let seen = false;
      for (let j = 0; j < uniq.length; j++) {
        if (uniq[j] === n) { seen = true; break; }
      }
      if (!seen) { uniq.push(n); }
    }
    uniq.sort();
    for (let i = 0; i < uniq.length; i++) {
      const n = uniq[i];
      if (n === "set-cookie") {
        const sc = this.getSetCookie();
        for (let j = 0; j < sc.length; j++) { out.push([n, sc[j]]); }
      } else {
        out.push([n, this.get(n)]);
      }
    }
    return out;
  }
  values(): string[] {
    const es = this.entries();
    const out: string[] = [];
    for (let i = 0; i < es.length; i++) { out.push(es[i][1]); }
    return out;
  }
  forEach(cb: (v: string, k: string, h: Headers) => void): void {
    const es = this.entries();
    for (let i = 0; i < es.length; i++) { cb(es[i][1], es[i][0], this); }
  }
  *[Symbol.iterator](): [string, string][] {
    const es = this.entries();
    for (let i = 0; i < es.length; i++) { yield es[i]; }
  }
}

class FormData {
  #names: string[] = [];
  #values: any[] = [];
  append(name: any, value: any): void {
    this.#names.push("" + name);
    this.#values.push("" + value);
  }
  delete(name: any): void {
    const n = "" + name;
    const keepN: string[] = [];
    const keepV: any[] = [];
    for (let i = 0; i < this.#names.length; i++) {
      if (this.#names[i] !== n) { keepN.push(this.#names[i]); keepV.push(this.#values[i]); }
    }
    this.#names = keepN;
    this.#values = keepV;
  }
  set(name: any, value: any): void {
    const n = "" + name;
    // `set` replaces the FIRST entry in place and drops the rest (spec).
    let first = -1;
    for (let i = 0; i < this.#names.length; i++) {
      if (this.#names[i] === n) { first = i; break; }
    }
    if (first < 0) { this.append(name, value); return; }
    const keepN: string[] = [];
    const keepV: any[] = [];
    for (let i = 0; i < this.#names.length; i++) {
      if (i === first) { keepN.push(n); keepV.push("" + value); }
      else if (this.#names[i] !== n) { keepN.push(this.#names[i]); keepV.push(this.#values[i]); }
    }
    this.#names = keepN;
    this.#values = keepV;
  }
  get(name: any): any {
    const n = "" + name;
    for (let i = 0; i < this.#names.length; i++) {
      if (this.#names[i] === n) return this.#values[i];
    }
    return null;
  }
  getAll(name: any): any[] {
    const n = "" + name;
    const out: any[] = [];
    for (let i = 0; i < this.#names.length; i++) {
      if (this.#names[i] === n) { out.push(this.#values[i]); }
    }
    return out;
  }
  has(name: any): boolean {
    const n = "" + name;
    for (let i = 0; i < this.#names.length; i++) {
      if (this.#names[i] === n) return true;
    }
    return false;
  }
  keys(): string[] {
    const out: string[] = [];
    for (let i = 0; i < this.#names.length; i++) { out.push(this.#names[i]); }
    return out;
  }
  values(): any[] {
    const out: any[] = [];
    for (let i = 0; i < this.#values.length; i++) { out.push(this.#values[i]); }
    return out;
  }
  entries(): [string, any][] {
    const out: [string, any][] = [];
    for (let i = 0; i < this.#names.length; i++) { out.push([this.#names[i], this.#values[i]]); }
    return out;
  }
  forEach(cb: (v: any, k: string, fd: FormData) => void): void {
    for (let i = 0; i < this.#names.length; i++) { cb(this.#values[i], this.#names[i], this); }
  }
  *[Symbol.iterator](): [string, any][] {
    for (let i = 0; i < this.#names.length; i++) { yield [this.#names[i], this.#values[i]]; }
  }
}

class Blob {
  // Parts normalized at construction: each part becomes a STRING (its decoded
  // text) — size/text derive from it. A string part stays as-is; a
  // Uint8Array-like part (`.length` + numeric indexing) is UTF-8-decoded.
  #text: string = "";
  #type: string = "";
  constructor(parts: any = undefined, opts: any = undefined) {
    if (parts !== undefined && parts !== null) {
      for (const p of parts) {
        if (typeof p === "string") { this.#text += p; }
        else { this.#text += __utf8_decode(p); }
      }
    }
    if (opts !== undefined && opts !== null) {
      const t = opts.type;
      if (t !== undefined) { this.#type = ("" + t).toLowerCase(); }
    }
  }
  get size(): number { return __utf8_len_str(this.#text); }
  get type(): string { return this.#type; }
  text(): string { return this.#text; }
  slice(start: number = 0, end: number = -1, contentType: any = undefined): Blob {
    let s = this.#text;
    // Blob.slice indexes BYTES; this interim slices code units of the decoded
    // text — exact for ASCII payloads.
    if (end === -1) { end = s.length; }
    const piece = s.slice(start, end);
    const b = new Blob([piece]);
    if (contentType !== undefined) { return new Blob([piece], { type: contentType }); }
    return b;
  }
}

class File extends Blob {
  name: string;
  lastModified: number;
  constructor(parts: any, name: any, opts: any = undefined) {
    super(parts, opts);
    this.name = "" + name;
    let lm = 0;
    if (opts !== undefined && opts !== null) {
      const v = opts.lastModified;
      if (v !== undefined) { lm = v; }
      else { lm = Date.now(); }
    } else {
      lm = Date.now();
    }
    this.lastModified = lm;
  }
}

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
