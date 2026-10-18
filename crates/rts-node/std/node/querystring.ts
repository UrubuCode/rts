// node:querystring, authored in TypeScript and compiled with the program.
//
// Replaces the native `crates/rts-node/src/querystring.rs`; behaviour follows
// Node's `lib/querystring.js` (checked against a real Node by differential
// test).

function hexValue(code: number): number {
  if (code >= 48 && code <= 57) return code - 48;
  if (code >= 65 && code <= 70) return code - 55;
  if (code >= 97 && code <= 102) return code - 87;
  return -1;
}

// The UTF-8 bytes of `text`, with `%XX` pairs decoded to their byte and (if
// asked) `+` read as a space. A lone surrogate is U+FFFD, as `Buffer.from`.
function unescapedBytes(text: string, decodeSpaces: boolean): number[] {
  const bytes: number[] = [];
  const length = text.length;
  let index = 0;
  while (index < length) {
    const code = text.charCodeAt(index);
    if (code === 37 && index + 2 < length && hexValue(text.charCodeAt(index + 1)) >= 0 && hexValue(text.charCodeAt(index + 2)) >= 0) {
      bytes.push(hexValue(text.charCodeAt(index + 1)) * 16 + hexValue(text.charCodeAt(index + 2)));
      index += 3;
      continue;
    }
    if (code === 43 && decodeSpaces) {
      bytes.push(32);
      index += 1;
      continue;
    }
    let point = code;
    index += 1;
    if (code >= 0xd800 && code <= 0xdbff && index < length) {
      const next = text.charCodeAt(index);
      if (next >= 0xdc00 && next <= 0xdfff) {
        point = 0x10000 + ((code - 0xd800) << 10) + (next - 0xdc00);
        index += 1;
      }
    }
    if (point >= 0xd800 && point <= 0xdfff) point = 0xfffd;
    if (point < 0x80) {
      bytes.push(point);
    } else if (point < 0x800) {
      bytes.push(0xc0 | (point >> 6), 0x80 | (point & 63));
    } else if (point < 0x10000) {
      bytes.push(0xe0 | (point >> 12), 0x80 | ((point >> 6) & 63), 0x80 | (point & 63));
    } else {
      bytes.push(0xf0 | (point >> 18), 0x80 | ((point >> 12) & 63), 0x80 | ((point >> 6) & 63), 0x80 | (point & 63));
    }
  }
  return bytes;
}

function unescapeWith(text: string, decodeSpaces: boolean): string {
  try {
    return decodeURIComponent(text);
  } catch (error) {
    return new TextDecoder().decode(new Uint8Array(unescapedBytes(text, decodeSpaces)));
  }
}

export function unescape(text: any, decodeSpaces?: boolean): string {
  return unescapeWith(String(text), decodeSpaces === true);
}

export function escape(text: any): string {
  if (typeof text !== "string") {
    text = typeof text === "object" ? String(text) : text + "";
  }
  return encodeURIComponent(text);
}

function finiteOrNone(value: any): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" && isFinite(value)) return "" + value;
  if (typeof value === "bigint") return "" + value;
  if (typeof value === "boolean") return value ? "true" : "false";
  return "";
}

function decodeText(text: string, decoder: any): string {
  try {
    return decoder(text);
  } catch (error) {
    return unescapeWith(text, true);
  }
}

export function parse(query: any, sep?: any, eq?: any, options?: any): any {
  const result: any = Object.create(null);
  if (typeof query !== "string" || query.length === 0) return result;
  const separator: string = sep ? sep + "" : "&";
  const equals: string = eq ? eq + "" : "=";
  let pairs = 1000;
  let custom: any = null;
  if (options) {
    if (typeof options.maxKeys === "number") pairs = options.maxKeys > 0 ? options.maxKeys : -1;
    if (typeof options.decodeURIComponent === "function") custom = options.decodeURIComponent;
  }
  let at = 0;
  const total = query.length;
  while (at <= total) {
    let stop = query.indexOf(separator, at);
    if (stop < 0) stop = total;
    if (stop > at) {
      const part = query.slice(at, stop);
      const split = part.indexOf(equals);
      let rawKey = part;
      let rawValue = "";
      if (split >= 0) {
        rawKey = part.slice(0, split);
        rawValue = part.slice(split + equals.length);
      }
      let key: string;
      let value: string;
      if (custom !== null) {
        // A caller-supplied decoder sees every non-empty part, with `+` as
        // `%20` (Node does exactly this).
        key = rawKey.length ? decodeText(rawKey.indexOf("+") >= 0 ? rawKey.split("+").join("%20") : rawKey, custom) : rawKey;
        value = rawValue.length ? decodeText(rawValue.indexOf("+") >= 0 ? rawValue.split("+").join("%20") : rawValue, custom) : rawValue;
      } else {
        key = rawKey.indexOf("+") >= 0 ? rawKey.split("+").join(" ") : rawKey;
        value = rawValue.indexOf("+") >= 0 ? rawValue.split("+").join(" ") : rawValue;
        if (key.indexOf("%") >= 0) key = unescapeWith(key, false);
        if (value.indexOf("%") >= 0) value = unescapeWith(value, false);
      }
      const held = result[key];
      if (held === undefined) {
        result[key] = value;
      } else if (Array.isArray(held)) {
        held.push(value);
      } else {
        result[key] = [held, value];
      }
    }
    // An empty part (`&&`) still spends one of `maxKeys`, as in Node.
    pairs -= 1;
    if (pairs === 0) return result;
    at = stop + separator.length;
  }
  return result;
}

export function stringify(object: any, sep?: any, eq?: any, options?: any): string {
  const separator = sep || "&";
  const equals = eq || "=";
  let custom: any = null;
  if (options && typeof options.encodeURIComponent === "function") custom = options.encodeURIComponent;
  const convert = (value: any): string => {
    if (custom !== null) return custom(finiteOrNone(value));
    if (typeof value === "string") return value.length ? escape(value) : "";
    if (typeof value === "number" && isFinite(value)) return escape("" + value);
    if (typeof value === "bigint") return "" + value;
    if (typeof value === "boolean") return value ? "true" : "false";
    return "";
  };
  if (object !== null && typeof object === "object") {
    const keys = Object.keys(object);
    let fields = "";
    for (let i = 0; i < keys.length; i++) {
      const k = keys[i];
      const v = object[k];
      const prefix = convert(k) + equals;
      if (Array.isArray(v)) {
        if (v.length === 0) continue;
        if (fields) fields += separator;
        for (let j = 0; j < v.length; j++) {
          if (j) fields += separator;
          fields += prefix + convert(v[j]);
        }
      } else {
        if (fields) fields += separator;
        fields += prefix + convert(v);
      }
    }
    return fields;
  }
  return "";
}

export const decode = parse;
export const encode = stringify;
export function unescapeBuffer(text: any, decodeSpaces?: boolean): any {
  return Buffer.from(unescapedBytes(String(text), decodeSpaces === true));
}

export default { unescapeBuffer, unescape, escape, stringify, encode, parse, decode };
