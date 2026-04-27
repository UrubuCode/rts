// fetch unificado — escolhe net (HTTP) ou tls (HTTPS) automaticamente.

import { net, tls, buffer, string, thread } from "rts";
import { ParsedUrl, parseUrl } from "./url";
import { Response } from "./response";
import { RequestInit, defaultInit } from "./request";

const RECV_BUFFER_SIZE = 8192;

export function fetch(url: string): Response {
  return fetchWith(url, defaultInit());
}

export function fetchWith(url: string, init: RequestInit): Response {
  const u = parseUrl(url);

  const addr = u.host + ":" + (u.port as unknown as string);
  const tcp = net.tcp_connect(addr);
  if (tcp == 0) {
    return new Response(0, "tcp_connect failed", "", "");
  }

  if (u.scheme == "https") {
    return doRequestTls(tcp, u, init);
  } else {
    return doRequestPlain(tcp, u, init);
  }
}

function buildRequestLine(u: ParsedUrl, init: RequestInit): string {
  let req = "";
  req = req + init.method + " " + u.path + " HTTP/1.1\r\n";
  req = req + "Host: " + u.host + "\r\n";
  req = req + "User-Agent: rts-http/0.1\r\n";
  req = req + "Accept: */*\r\n";
  req = req + "Connection: close\r\n";
  if (string.byte_len(init.headers) > 0) {
    req = req + init.headers;
  }
  if (string.byte_len(init.body) > 0) {
    req = req + "Content-Length: " + (string.byte_len(init.body) as unknown as string) + "\r\n";
  }
  req = req + "\r\n";
  if (string.byte_len(init.body) > 0) {
    req = req + init.body;
  }
  return req;
}

function doRequestPlain(tcp: number, u: ParsedUrl, init: RequestInit): Response {
  const req = buildRequestLine(u, init);
  const sent = net.tcp_send(tcp, req);
  if (sent < 0) {
    net.tcp_close(tcp);
    return new Response(0, "tcp_send failed", "", "");
  }

  thread.sleep_ms(300);

  const buf = buffer.alloc_zeroed(RECV_BUFFER_SIZE);
  const n = net.tcp_recv(tcp, buffer.ptr(buf), RECV_BUFFER_SIZE);
  net.tcp_close(tcp);

  if (n <= 0) {
    buffer.free(buf);
    return new Response(0, "tcp_recv failed", "", "");
  }

  const raw = buffer.to_string(buf);
  buffer.free(buf);
  return parseResponse(raw, n);
}

function doRequestTls(tcp: number, u: ParsedUrl, init: RequestInit): Response {
  const stream = tls.client(tcp, u.host);
  if (stream == 0) {
    return new Response(0, "tls handshake failed", "", "");
  }

  const req = buildRequestLine(u, init);
  const sent = tls.send(stream, req);
  if (sent < 0) {
    tls.close(stream);
    return new Response(0, "tls.send failed", "", "");
  }

  thread.sleep_ms(500);

  const buf = buffer.alloc_zeroed(RECV_BUFFER_SIZE);
  const n = tls.recv(stream, buffer.ptr(buf), RECV_BUFFER_SIZE);
  tls.close(stream);

  if (n <= 0) {
    buffer.free(buf);
    return new Response(0, "tls.recv failed", "", "");
  }

  const raw = buffer.to_string(buf);
  buffer.free(buf);
  return parseResponse(raw, n);
}

function parseResponse(raw: string, totalBytes: number): Response {
  // status line: "HTTP/1.1 NNN REASON\r\n"
  const eol1 = string.find(raw, "\r\n");
  if (eol1 < 0) {
    return new Response(0, "no status line", "", "");
  }
  const statusLine = sliceTo(raw, eol1);
  const sp1 = string.find(statusLine, " ");
  const rest = sliceFrom(statusLine, sp1 + 1);
  const sp2 = string.find(rest, " ");
  const codeStr = sliceTo(rest, sp2);
  const reason = sliceFrom(rest, sp2 + 1);
  const status = parseIntStr(codeStr);

  // headers
  const sep = string.find(raw, "\r\n\r\n");
  if (sep < 0) {
    return new Response(status, reason, "", "");
  }
  const headersRaw = sliceFromTo(raw, eol1 + 2, sep);

  // body
  const bodyStart = sep + 4;
  const body = sliceFromTo(raw, bodyStart, totalBytes);

  return new Response(status, reason, headersRaw, body);
}

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
