// JSON — the rts-shared stdlib utility (NOT a primordial; no native syntax). Pure
// TS over primordials only (typeof / === / Object.keys / Array.isArray / String /
// recursion) — the engine names NOTHING here; it just runs the generic primitives.
// `JSON.stringify` / `JSON.parse` are static methods of this ambient class.

class JSON {
  // JSON.stringify(value, replacer?, space?). `replacer` is ignored (only the
  // common null/undefined forms appear in practice); `space` (a number) selects
  // pretty output with that many spaces of indent.
  static stringify(value: any, replacer?: any, space?: any): any {
    const indent: number = typeof space === "number" ? space : 0;
    return __json_render(value, indent, 0);
  }

  // JSON.parse(text) — recursive-descent parser over the primordials. Returns the
  // parsed value (objects become plain object literals, arrays plain arrays).
  static parse(text: string): any {
    const p = new __JsonParser(text);
    const v = p.parseValue();
    p.skipWs();
    return v;
  }
}

// ── stringify helpers (plain functions; the engine runs them generically) ──────

function __json_render(v: any, indent: number, depth: number): any {
  if (v === null) return "null";
  const t = typeof v;
  if (t === "number") return isFinite(v) ? ("" + v) : "null";
  if (t === "boolean") return v ? "true" : "false";
  if (t === "string") return __json_quote(v);
  // undefined / function: omitted by JSON (the caller turns this into `null` in an
  // array, or skips the key in an object, or returns undefined at top level).
  if (t === "undefined" || t === "function") return undefined;
  if (Array.isArray(v)) {
    if (v.length === 0) return "[]";
    const parts: string[] = [];
    for (let i = 0; i < v.length; i++) {
      let s: any = __json_render(v[i], indent, depth + 1);
      if (s === undefined) s = "null";
      parts.push(s);
    }
    return __json_wrap("[", "]", parts, indent, depth);
  }
  // plain object: enumerate own keys, omit undefined/function-valued members.
  const keys = Object.keys(v);
  const parts: string[] = [];
  const colon = indent > 0 ? ": " : ":";
  for (let i = 0; i < keys.length; i++) {
    const val: any = __json_render(v[keys[i]], indent, depth + 1);
    if (val !== undefined) {
      parts.push(__json_quote(keys[i]) + colon + val);
    }
  }
  if (parts.length === 0) return "{}";
  return __json_wrap("{", "}", parts, indent, depth);
}

// Join `parts` between brackets: compact (indent 0) `[a,b]`, or pretty with one
// member per indented line.
function __json_wrap(open: string, close: string, parts: string[], indent: number, depth: number): string {
  if (indent === 0) {
    let body = "";
    for (let i = 0; i < parts.length; i++) {
      if (i > 0) body += ",";
      body += parts[i];
    }
    return open + body + close;
  }
  const inner = __json_spaces(indent * (depth + 1));
  const outer = __json_spaces(indent * depth);
  let body = "";
  for (let i = 0; i < parts.length; i++) {
    if (i > 0) body += ",\n";
    body += inner + parts[i];
  }
  return open + "\n" + body + "\n" + outer + close;
}

function __json_spaces(n: number): string {
  let s = "";
  for (let i = 0; i < n; i++) s += " ";
  return s;
}

// Double-quote a string with the JSON escape set.
function __json_quote(s: string): string {
  let out = '"';
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
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
    let out = "";
    while (this.#i < this.#n) {
      const c = this.#s[this.#i];
      this.#i++;
      if (c === '"') break;
      if (c === "\\") {
        const e = this.#s[this.#i];
        this.#i++;
        if (e === "n") out += "\n";
        else if (e === "t") out += "\t";
        else if (e === "r") out += "\r";
        else out += e;
      } else {
        out += c;
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
