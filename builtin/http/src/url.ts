// parseUrl — quebra "scheme://host[:port][/path]" em partes.
//
// Limitacoes:
//   - sem suporte a userinfo (user:pass@host)
//   - sem suporte a fragment (#frag)
//   - query e mantida no path (path inclui ?...)

import { string } from "rts";

export class ParsedUrl {
  scheme: string;
  host: string;
  port: number;
  path: string;
  constructor(scheme: string, host: string, port: number, path: string) {
    this.scheme = scheme;
    this.host = host;
    this.port = port;
    this.path = path;
  }
}

export function parseUrl(url: string): ParsedUrl {
  // scheme
  const sepScheme = string.find(url, "://");
  if (sepScheme < 0) {
    // url relativa? tratamos como http://url
    return new ParsedUrl("http", url, 80, "/");
  }
  const scheme = sliceTo(url, sepScheme);
  const rest = sliceFrom(url, sepScheme + 3);

  // host[:port]/path
  const slash = string.find(rest, "/");
  let hostPort = rest;
  let path = "/";
  if (slash >= 0) {
    hostPort = sliceTo(rest, slash);
    path = sliceFrom(rest, slash);
  }

  let host = hostPort;
  let port = scheme == "https" ? 443 : 80;
  const colon = string.find(hostPort, ":");
  if (colon >= 0) {
    host = sliceTo(hostPort, colon);
    const portStr = sliceFrom(hostPort, colon + 1);
    port = parseIntStr(portStr);
  }

  return new ParsedUrl(scheme, host, port, path);
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

function parseIntStr(s: string): number {
  let n = 0;
  const len = string.char_count(s);
  for (let i = 0; i < len; i = i + 1) {
    const c = string.char_code_at(s, i);
    if (c < 48 || c > 57) break;  // nao-digito interrompe
    n = n * 10 + (c - 48);
  }
  return n;
}
