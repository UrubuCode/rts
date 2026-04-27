// Response — encapsula status code, headers, body recebido.

import { string } from "rts";

export class Response {
  status: number;
  statusText: string;
  headersRaw: string;
  body: string;

  constructor(status: number, statusText: string, headersRaw: string, body: string) {
    this.status = status;
    this.statusText = statusText;
    this.headersRaw = headersRaw;
    this.body = body;
  }

  // Le um header pelo nome (case-insensitive). Retorna "" se nao existe.
  header(name: string): string {
    const lowered = string.to_lower(this.headersRaw);
    const target = string.to_lower(name) + ":";
    const idx = string.find(lowered, target);
    if (idx < 0) return "";
    // Garante que esta no inicio de linha (idx == 0 ou char anterior == \n).
    if (idx > 0) {
      const prev = string.char_code_at(this.headersRaw, idx - 1);
      if (prev != 10) return "";  // \n = 10
    }
    // Acha o valor: depois do : ate \r ou \n
    const valStart = idx + string.char_count(target);
    const tail = sliceFrom(this.headersRaw, valStart);
    const eol = string.find(tail, "\r");
    const value = eol >= 0 ? sliceTo(tail, eol) : tail;
    return string.trim(value);
  }

  text(): string {
    return this.body;
  }

  // ok = status na faixa 200-299
  ok(): boolean {
    return this.status >= 200 && this.status < 300;
  }
}

function sliceTo(s: string, end: number): string {
  let out = "";
  for (let i = 0; i < end; i = i + 1) out = out + string.char_at(s, i);
  return out;
}

function sliceFrom(s: string, start: number): string {
  let out = "";
  const n = string.char_count(s);
  for (let i = start; i < n; i = i + 1) out = out + string.char_at(s, i);
  return out;
}
