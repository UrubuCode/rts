// node:string_decoder, authored in TypeScript and compiled with the program.
//
// Replaces the native `crates/rts-node/src/string_decoder.rs`; behaviour follows
// Node's `src/string_decoder.cc` (checked against a real Node by differential
// test): up to four bytes of an unfinished character are held back between
// `write` calls.

const UTF8 = 1;
const UCS2 = 2;
const BASE64 = 3;
const OTHER = 0;

function normalize(encoding: any): string {
  if (encoding === undefined || encoding === null || encoding === "") return "utf8";
  const lower = typeof encoding === "string" ? encoding.toLowerCase() : "";
  switch (lower) {
    case "utf8":
    case "utf-8":
      return "utf8";
    case "ucs2":
    case "ucs-2":
    case "utf16le":
    case "utf-16le":
      return "utf16le";
    case "latin1":
    case "binary":
      return "latin1";
    case "base64":
    case "base64url":
    case "ascii":
    case "hex":
      return lower;
  }
  const error: any = new TypeError("Unknown encoding: " + String(encoding));
  error.code = "ERR_UNKNOWN_ENCODING";
  throw error;
}

function received(value: any): string {
  if (value === undefined) return "Received undefined";
  if (value === null) return "Received null";
  if (typeof value === "function") return "Received function " + value.name;
  if (typeof value === "object") {
    const ctor = value.constructor;
    if (ctor && ctor.name) return "Received an instance of " + ctor.name;
    return "Received " + String(value);
  }
  let shown: string = typeof value === "string" ? value : String(value);
  if (typeof value === "string") {
    if (shown.length > 28) shown = shown.slice(0, 25) + "...";
    shown = "'" + shown + "'";
  } else if (typeof value === "bigint") {
    shown = shown + "n";
  }
  return "Received type " + typeof value + " (" + shown + ")";
}

function bytesOf(value: any): Uint8Array {
  if (value instanceof Uint8Array) return value;
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array((value as any).buffer, (value as any).byteOffset, (value as any).byteLength);
  }
  const error: any = new TypeError(
    'The "buf" argument must be an instance of Buffer, TypedArray, or DataView. ' + received(value),
  );
  error.code = "ERR_INVALID_ARG_TYPE";
  throw error;
}

function text(bytes: Uint8Array, start: number, end: number, encoding: string): string {
  if (end <= start) return "";
  if (encoding === "utf16le") {
    let out = "";
    for (let i = start; i + 1 < end; i += 2) out += String.fromCharCode(bytes[i] | (bytes[i + 1] << 8));
    return out;
  }
  return Buffer.from(bytes.subarray(start, end)).toString(encoding as any);
}

export class StringDecoder {
  encoding: string;
  private kind: number;
  private held: number[];
  private missing: number;

  constructor(encoding?: any) {
    this.encoding = normalize(encoding);
    this.kind = this.encoding === "utf8" ? UTF8 : this.encoding === "utf16le" ? UCS2 : this.encoding === "base64" || this.encoding === "base64url" ? BASE64 : OTHER;
    this.held = [];
    this.missing = 0;
  }

  get lastChar(): any {
    const out = Buffer.alloc(4);
    for (let i = 0; i < this.held.length; i++) out[i] = this.held[i];
    return out;
  }

  get lastNeed(): number {
    return this.missing;
  }

  get lastTotal(): number {
    return this.held.length + this.missing;
  }

  write(input: any): string {
    if (typeof input === "string") return input;
    let data = bytesOf(input);
    const encoding = this.encoding;
    if (this.kind === OTHER) return text(data, 0, data.length, encoding);

    let nread = data.length;
    let start = 0;
    let prepend = "";
    const held = this.held;

    if (this.missing > 0) {
      if (this.kind === UTF8) {
        const limit = nread < this.missing ? nread : this.missing;
        for (let i = 0; i < limit; i++) {
          if ((data[i] & 0xc0) !== 0x80) {
            // Not a continuation byte: the held character is cut short.
            this.missing = 0;
            for (let j = 0; j < i; j++) held.push(data[j]);
            start = i;
            nread -= i;
            break;
          }
        }
      }
      const found = nread < this.missing ? nread : this.missing;
      for (let j = 0; j < found; j++) held.push(data[start + j]);
      start += found;
      nread -= found;
      this.missing -= found;
      if (this.missing === 0) {
        prepend = text(Uint8Array.from(held), 0, held.length, encoding);
        held.length = 0;
      }
    }

    if (nread === 0) return prepend;

    const end = start + nread;
    let buffered = 0;
    if (this.kind === UTF8) {
      if ((data[end - 1] & 0x80) !== 0) {
        for (let i = end - 1; ; i--) {
          buffered++;
          if ((data[i] & 0xc0) === 0x80) {
            if (buffered >= 4 || i === start) {
              buffered = 0;
              break;
            }
          } else {
            let need = 0;
            if ((data[i] & 0xe0) === 0xc0) need = 2;
            else if ((data[i] & 0xf0) === 0xe0) need = 3;
            else if ((data[i] & 0xf8) === 0xf0) need = 4;
            else {
              buffered = 0;
              break;
            }
            if (buffered >= need) {
              need = 0;
              buffered = 0;
            }
            this.missing = need - buffered;
            if (need === 0) this.missing = 0;
            break;
          }
        }
      }
    } else if (this.kind === UCS2) {
      if (nread % 2 === 1) {
        buffered = 1;
        this.missing = 1;
      } else if ((data[end - 1] & 0xfc) === 0xd8) {
        buffered = 2;
        this.missing = 2;
      }
    } else {
      buffered = nread % 3;
      if (buffered > 0) this.missing = 3 - buffered;
    }

    if (buffered > 0) {
      for (let j = 0; j < buffered; j++) held.push(data[end - buffered + j]);
    }
    return prepend + text(data, start, end - buffered, encoding);
  }

  end(input?: any): string {
    let result = "";
    if (input !== undefined) result = this.write(input);
    const held = this.held;
    if (this.kind === UCS2 && held.length % 2 === 1) held.pop();
    if (held.length > 0) {
      result += text(Uint8Array.from(held), 0, held.length, this.encoding);
    }
    held.length = 0;
    this.missing = 0;
    return result;
  }

  text(input: any, offset: number): string {
    const data = bytesOf(input);
    return this.write(data.subarray(offset));
  }
}

export default { StringDecoder };
