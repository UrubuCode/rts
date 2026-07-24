// JSON — the rts-shared stdlib utility (NOT a primordial; no native syntax). Pure
// TS over primordials only (typeof / === / Object.keys / Array.isArray / String /
// recursion) — the engine names NOTHING here; it just runs the generic primitives.
// `JSON.stringify` / `JSON.parse` are static methods of this ambient class.

class JSON {
  // JSON.stringify(value, replacer?, space?). `replacer`: a FUNCTION is applied
  // to every (key, value) pair top-down (the root under key ""); an ARRAY of
  // keys filters object properties. `space`: a number (0..10 spaces) or a
  // string (first 10 chars) selects pretty output.
  static stringify(value: any, replacer?: any, space?: any): any {
    let pad = "";
    if (typeof space === "number") {
      const n = space < 10 ? space : 10;
      for (let i = 0; i < n; i++) pad += " ";
    } else if (typeof space === "string") {
      for (let i = 0; i < space.length && i < 10; i++) pad += space[i];
    }
    let keyFilter: any = null;
    let fnReplacer: any = null;
    if (Array.isArray(replacer)) keyFilter = replacer;
    else if (typeof replacer === "function") fnReplacer = replacer;
    // Spec order per property: toJSON FIRST, then the replacer fn.
    let root: any = __json_tojson(value, "");
    if (fnReplacer !== null) root = fnReplacer("", root);
    const seen: any[] = [];
    return __json_render(root, pad, 0, keyFilter, fnReplacer, seen);
  }

  // JSON.parse(text[, reviver]) — recursive-descent parser over the primordials.
  // With a `reviver`, the parsed tree is walked BOTTOM-UP (ECMAScript
  // InternalizeJSONProperty): each property's value is replaced by
  // `reviver(key, value)`; a returned `undefined` REMOVES the property. The root
  // is wrapped in a `{ "": root }` holder and revived under key "".
  static parse(text: string, reviver?: any): any {
    const p = new __JsonParser(text);
    const v = p.parseValue();
    p.skipWs();
    if (typeof reviver === "function") {
      return __jsonRevive(v, "", reviver);
    }
    return v;
  }
}

// Walk `value` bottom-up, applying `reviver(key, child)` to every nested property
// (objects) / element (arrays) first, then to `value` itself under `key`. A child
// that revives to `undefined` is dropped (object: rebuilt without the key; array:
// the slot is set to `undefined`, matching the spec's element behavior). `value`'s
// final transform is `reviver(key, value)`.
function __jsonRevive(value: any, key: string, reviver: any): any {
  if (Array.isArray(value)) {
    for (let i = 0; i < value.length; i++) {
      const revived = __jsonRevive(value[i], "" + i, reviver);
      value[i] = revived; // a deleted array element becomes `undefined` (spec)
    }
  } else if (value !== null && typeof value === "object") {
    const keys = Object.keys(value);
    // Rebuild without the keys whose revived value is `undefined` (no `delete`).
    const out: any = {};
    for (let i = 0; i < keys.length; i++) {
      const k = keys[i];
      const revived = __jsonRevive(value[k], k, reviver);
      if (revived !== undefined) {
        out[k] = revived;
      }
    }
    value = out;
  }
  return reviver(key, value);
}

// ── stringify helpers (plain functions; the engine runs them generically) ──────

// SerializeJSONProperty's toJSON hook: an object value with a callable `toJSON`
// serializes its RESULT (called with the property key, spec); a Date serializes
// its ISO string (Date.prototype.toJSON — reached via instanceof because the
// Registry method is not a readable data slot). Runs BEFORE the replacer fn.
function __json_tojson(v: any, key: string): any {
  if (v !== null && typeof v === "object") {
    if (typeof v.toJSON === "function") return v.toJSON(key);
    if (v instanceof Date) return v.toISOString();
  }
  return v;
}

function __json_render(v: any, pad: string, depth: number, keyFilter: any, fnReplacer: any, seen: any[]): any {
  if (v === null) return "null";
  const t = typeof v;
  if (t === "number") return isFinite(v) ? ("" + v) : "null";
  if (t === "boolean") return v ? "true" : "false";
  if (t === "string") return __json_quote(v);
  // undefined / function: omitted by JSON (the caller turns this into `null` in an
  // array, or skips the key in an object, or returns undefined at top level).
  if (t === "undefined" || t === "function") return undefined;
  // A Boolean/Number/String WRAPPER object serializes as its wrapped PRIMITIVE
  // (SerializeJSONProperty reads [[BooleanData]]/ToNumber/ToString of the box) —
  // expando properties are ignored. `__prim` is the engine-owned wrapper slot.
  if (t === "object" && typeof v.__prim !== "undefined") {
    return __json_render(v.__prim, pad, depth, keyFilter, fnReplacer, seen);
  }
  // Cycle detection (ECMAScript SerializeJSON*): re-entering a value already on
  // the render stack throws.
  if (seen.indexOf(v) >= 0) {
    throw new TypeError("Converting circular structure to JSON");
  }
  seen.push(v);
  let out: any;
  if (Array.isArray(v)) {
    if (v.length === 0) {
      out = "[]";
    } else {
      const parts: string[] = [];
      for (let i = 0; i < v.length; i++) {
        let el: any = __json_tojson(v[i], "" + i);
        if (fnReplacer !== null) el = fnReplacer("" + i, el);
        let s: any = __json_render(el, pad, depth + 1, keyFilter, fnReplacer, seen);
        if (s === undefined) s = "null";
        parts.push(s);
      }
      out = __json_wrap("[", "]", parts, pad, depth);
    }
  } else {
    // plain object: enumerate own keys (an ARRAY replacer filters + orders
    // them — string/number entries and String/Number wrappers only, dedup),
    // apply the fn replacer per property, omit undefined/function members.
    let keys: any = Object.keys(v);
    if (keyFilter !== null) {
      const own = Object.keys(v);
      const picked: string[] = [];
      for (let i = 0; i < keyFilter.length; i++) {
        const kv: any = keyFilter[i];
        let k: any = undefined;
        if (typeof kv === "string") k = kv;
        else if (typeof kv === "number") k = "" + kv;
        else if (kv !== null && typeof kv === "object") {
          const p: any = kv.__prim;
          if (typeof p === "string") k = p;
          else if (typeof p === "number") k = "" + p;
        }
        if (k !== undefined && own.indexOf(k) >= 0 && picked.indexOf(k) < 0) picked.push(k);
      }
      keys = picked;
    }
    const parts: string[] = [];
    const colon = pad.length > 0 ? ": " : ":";
    for (let i = 0; i < keys.length; i++) {
      let pv: any = __json_tojson(v[keys[i]], keys[i]);
      if (fnReplacer !== null) pv = fnReplacer(keys[i], pv);
      const val: any = __json_render(pv, pad, depth + 1, keyFilter, fnReplacer, seen);
      if (val !== undefined) {
        parts.push(__json_quote(keys[i]) + colon + val);
      }
    }
    out = parts.length === 0 ? "{}" : __json_wrap("{", "}", parts, pad, depth);
  }
  seen.pop();
  return out;
}

// Join `parts` between brackets: compact (no pad) `[a,b]`, or pretty with one
// member per pad-indented line.
function __json_wrap(open: string, close: string, parts: string[], pad: string, depth: number): string {
  // `join` concatena de UMA vez. O laço `body += parts[i]` copiava todo o
  // acumulado a cada item — O(total²) ao serializar um array/objeto grande.
  if (pad.length === 0) {
    return open + parts.join(",") + close;
  }
  const inner = __json_repeat(pad, depth + 1);
  const outer = __json_repeat(pad, depth);
  const indented: string[] = [];
  for (let i = 0; i < parts.length; i++) indented.push(inner + parts[i]);
  return open + "\n" + indented.join(",\n") + "\n" + outer + close;
}

function __json_repeat(pad: string, n: number): string {
  let s = "";
  for (let i = 0; i < n; i++) s += pad;
  return s;
}

// Double-quote a string with the JSON escape set. Iterates by CODE POINTS
// (for-of), never by UTF-16 unit index: `s[i]` on half a surrogate pair reads
// a LONE surrogate the engine's UTF-8 pool cannot hold (it became U+FFFD —
// JSON.stringify("😀") printed replacement chars).
function __json_quote(s: string): string {
  let out = '"';
  for (const c of s) {
    if (c === '"') out += '\\"';
    else if (c === "\\") out += "\\\\";
    else if (c === "\n") out += "\\n";
    else if (c === "\r") out += "\\r";
    else if (c === "\t") out += "\\t";
    else out += c;
  }
  return out + '"';
}

// ── parse: a small recursive-descent parser ───────────────────────────────────

class __JsonParser {
  #s: string;
  #i: number;
  #n: number;
  constructor(s: string) {
    this.#s = s;
    this.#i = 0;
    this.#n = s.length;
  }
  skipWs(): void {
    while (this.#i < this.#n) {
      const c = this.#s[this.#i];
      if (c === " " || c === "\t" || c === "\n" || c === "\r") this.#i++;
      else break;
    }
  }
  parseValue(): any {
    this.skipWs();
    const c = this.#s[this.#i];
    if (c === "{") return this.parseObject();
    if (c === "[") return this.parseArray();
    if (c === '"') return this.parseString();
    if (c === "t") { this.#i += 4; return true; }
    if (c === "f") { this.#i += 5; return false; }
    if (c === "n") { this.#i += 4; return null; }
    return this.parseNumber();
  }
  parseObject(): any {
    this.#i++; // {
    const obj: any = {};
    this.skipWs();
    if (this.#s[this.#i] === "}") { this.#i++; return obj; }
    while (this.#i < this.#n) {
      this.skipWs();
      const key = this.parseString();
      this.skipWs();
      if (this.#s[this.#i] !== ":") throw new Error("Expected ':'");
      this.#i++; // :
      obj[key] = this.parseValue();
      this.skipWs();
      if (this.#s[this.#i] === ",") {
        this.#i++;
        continue;
      }
      if (this.#s[this.#i] === "}") {
        this.#i++;
        break;
      }
      throw new Error("Expected ',' or '}'");
    }
    return obj;
  }
  parseArray(): any {
    this.#i++; // [
    const arr: any[] = [];
    this.skipWs();
    if (this.#s[this.#i] === "]") { this.#i++; return arr; }
    while (this.#i < this.#n) {
      arr.push(this.parseValue());
      this.skipWs();
      if (this.#s[this.#i] === ",") {
        this.#i++;
        continue;
      }
      if (this.#s[this.#i] === "]") {
        this.#i++;
        break;
      }
      throw new Error("Expected ',' or ']'");
    }
    return arr;
  }
  parseString(): string {
    this.#i++; // opening quote
    // FAST PATH: most JSON strings carry no escape at all. Scan to the closing
    // quote and return ONE slice. The old `out += c` loop copied the whole
    // accumulator per character — O(k²) per string, and the dominant cost of
    // parsing a large document once indexing itself became O(1).
    const start = this.#i;
    let j = this.#i;
    while (j < this.#n) {
      const c = this.#s[j];
      if (c === '"') {
        this.#i = j + 1;
        return this.#s.substring(start, j);
      }
      if (c === "\\") break; // has an escape → slow path below
      j++;
    }
    // SLOW PATH (escapes present): still avoids per-character concatenation by
    // appending each escape-free RUN in one go.
    let out = this.#s.substring(start, j);
    this.#i = j;
    while (this.#i < this.#n) {
      const c = this.#s[this.#i];
      if (c === '"') {
        this.#i++;
        break;
      }
      if (c === "\\") {
        this.#i++;
        const e = this.#s[this.#i];
        this.#i++;
        if (e === "n") out += "\n";
        else if (e === "t") out += "\t";
        else if (e === "r") out += "\r";
        else out += e;
      } else {
        const runStart = this.#i;
        let k = this.#i;
        while (k < this.#n) {
          const d = this.#s[k];
          if (d === '"' || d === "\\") break;
          k++;
        }
        out += this.#s.substring(runStart, k);
        this.#i = k;
      }
    }
    return out;
  }
  parseNumber(): number {
    let start = this.#i;
    while (this.#i < this.#n) {
      const c = this.#s[this.#i];
      if (c === "-" || c === "+" || c === "." || c === "e" || c === "E" || (c >= "0" && c <= "9")) {
        this.#i++;
      } else break;
    }
    return Number(this.#s.substring(start, this.#i));
  }
}
