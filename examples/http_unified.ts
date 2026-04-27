// Cliente HTTP/HTTPS unificado em arquivo unico.
// Inline da logica do builtin/http/ — quando o resolver de packages
// builtin estiver pronto, vamos usar import { fetch } from "http".
//
// Uso:
//   target/release/rts.exe run examples/http_unified.ts

import { net, tls, buffer, string, thread, io } from "rts";

const RECV_BUFFER_SIZE = 8192;

class ParsedUrl {
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

class Response {
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
  ok(): boolean { return this.status >= 200 && this.status < 300; }
}

function fetch(url: string): Response {
  const u = parseUrl(url);
  const addr = u.host + ":" + (u.port as unknown as string);
  const tcp = net.tcp_connect(addr);
  if (tcp == 0) {
    return new Response(0, "tcp_connect failed", "", "");
  }

  if (u.scheme == "https") {
    return doRequestTls(tcp, u);
  } else {
    return doRequestPlain(tcp, u);
  }
}

function buildRequestLine(u: ParsedUrl): string {
  let req = "";
  req = req + "GET " + u.path + " HTTP/1.1\r\n";
  req = req + "Host: " + u.host + "\r\n";
  req = req + "User-Agent: rts-http/0.1\r\n";
  req = req + "Accept: */*\r\n";
  req = req + "Connection: close\r\n";
  req = req + "\r\n";
  return req;
}

function doRequestPlain(tcp: number, u: ParsedUrl): Response {
  const req = buildRequestLine(u);
  const sent = net.tcp_send(tcp, req);
  if (sent < 0) { net.tcp_close(tcp); return new Response(0, "tcp_send failed", "", ""); }
  thread.sleep_ms(300);
  const buf = buffer.alloc_zeroed(RECV_BUFFER_SIZE);
  const n = net.tcp_recv(tcp, buffer.ptr(buf), RECV_BUFFER_SIZE);
  net.tcp_close(tcp);
  if (n <= 0) { buffer.free(buf); return new Response(0, "tcp_recv failed", "", ""); }
  const raw = buffer.to_string(buf);
  buffer.free(buf);
  return parseResponse(raw, n);
}

function doRequestTls(tcp: number, u: ParsedUrl): Response {
  const stream = tls.client(tcp, u.host);
  if (stream == 0) return new Response(0, "tls handshake failed", "", "");
  const req = buildRequestLine(u);
  const sent = tls.send(stream, req);
  if (sent < 0) { tls.close(stream); return new Response(0, "tls.send failed", "", ""); }
  thread.sleep_ms(500);
  const buf = buffer.alloc_zeroed(RECV_BUFFER_SIZE);
  const n = tls.recv(stream, buffer.ptr(buf), RECV_BUFFER_SIZE);
  tls.close(stream);
  if (n <= 0) { buffer.free(buf); return new Response(0, "tls.recv failed", "", ""); }
  const raw = buffer.to_string(buf);
  buffer.free(buf);
  return parseResponse(raw, n);
}

function parseResponse(raw: string, totalBytes: number): Response {
  const eol1 = string.find(raw, "\r\n");
  if (eol1 < 0) return new Response(0, "no status line", "", "");
  const statusLine = sliceTo(raw, eol1);
  const sp1 = string.find(statusLine, " ");
  const rest = sliceFrom(statusLine, sp1 + 1);
  const sp2 = string.find(rest, " ");
  const codeStr = sliceTo(rest, sp2);
  const reason = sliceFrom(rest, sp2 + 1);
  const status = parseIntStr(codeStr);
  const sep = string.find(raw, "\r\n\r\n");
  if (sep < 0) return new Response(status, reason, "", "");
  const headersRaw = sliceFromTo(raw, eol1 + 2, sep);
  const bodyStart = sep + 4;
  const body = sliceFromTo(raw, bodyStart, totalBytes);
  return new Response(status, reason, headersRaw, body);
}

function parseUrl(url: string): ParsedUrl {
  const sepScheme = string.find(url, "://");
  if (sepScheme < 0) return new ParsedUrl("http", url, 80, "/");
  const scheme = sliceTo(url, sepScheme);
  const rest = sliceFrom(url, sepScheme + 3);
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
    port = parseIntStr(sliceFrom(hostPort, colon + 1));
  }
  return new ParsedUrl(scheme, host, port, path);
}

function show(url: string): void {
  io.print("");
  io.print("→ " + url);
  const r = fetch(url);
  io.print("  status: " + r.status + " " + r.statusText);
  // OBS: string.byte_len em vez de .length (strings vindas de buffer
  // podem conter \0 e .length para na primeira ocorrencia — bug #235).
  const bodyLen = string.byte_len(r.body);
  io.print("  body length: " + bodyLen + " bytes");
  if (bodyLen > 0) {
    const previewLen = bodyLen > 200 ? 200 : bodyLen;
    io.print("  body preview: " + sliceTo(r.body, previewLen));
  }
}

show("http://httpforever.com/");
show("https://api.github.com/");
io.print("");
io.print("✓ mesma API, dois protocolos");

function parseIntStr(s: string): number {
  let n = 0;
  const len = string.char_count(s);
  for (let i = 0; i < len; i = i + 1) {
    const c = string.char_code_at(s, i);
    if (c < 48 || c > 57) break;
    n = n * 10 + (c - 48);
  }
  return n;
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
function sliceFromTo(s: string, start: number, end: number): string {
  let out = "";
  for (let i = start; i < end; i = i + 1) out = out + string.char_at(s, i);
  return out;
}
