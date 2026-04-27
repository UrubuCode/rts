// fetch() unificado HTTP/HTTPS — mesma API resolve os dois schemes.
import { describe, test, expect } from "rts:test";
import { net, tls, buffer, string, thread } from "rts";

// Inline da mesma logica do builtin/http (resolver de packages
// builtin nao auto-detecta builtin/ ainda).

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
  body: string;
  bodyLen: number;
  constructor(status: number, body: string, bodyLen: number) {
    this.status = status;
    this.body = body;
    this.bodyLen = bodyLen;
  }
}

function parseUrl(url: string): ParsedUrl {
  const sepScheme = string.find(url, "://");
  const scheme = sliceTo(url, sepScheme);
  const rest = sliceFrom(url, sepScheme + 3);
  const slash = string.find(rest, "/");
  const hostPort = slash >= 0 ? sliceTo(rest, slash) : rest;
  const path = slash >= 0 ? sliceFrom(rest, slash) : "/";
  return new ParsedUrl(scheme, hostPort, scheme == "https" ? 443 : 80, path);
}

function fetchUrl(url: string): Response {
  const u = parseUrl(url);
  const tcp = net.tcp_connect(u.host + ":" + (u.port as unknown as string));
  if (tcp == 0) return new Response(0, "", 0);

  let req = "";
  req = req + "GET " + u.path + " HTTP/1.1\r\n";
  req = req + "Host: " + u.host + "\r\n";
  req = req + "User-Agent: rts-test/0.1\r\n";
  req = req + "Connection: close\r\n\r\n";

  const buf = buffer.alloc_zeroed(8192);
  let n = 0;

  if (u.scheme == "https") {
    const stream = tls.client(tcp, u.host);
    if (stream == 0) { buffer.free(buf); return new Response(0, "", 0); }
    tls.send(stream, req);
    thread.sleep_ms(500);
    n = tls.recv(stream, buffer.ptr(buf), 8192);
    tls.close(stream);
  } else {
    net.tcp_send(tcp, req);
    thread.sleep_ms(300);
    n = net.tcp_recv(tcp, buffer.ptr(buf), 8192);
    net.tcp_close(tcp);
  }

  if (n <= 0) { buffer.free(buf); return new Response(0, "", 0); }

  const raw = buffer.to_string(buf);
  buffer.free(buf);
  // status code: chars 9..12 de "HTTP/1.1 200 OK"
  let status = 0;
  for (let i = 9; i < 12; i = i + 1) {
    const c = string.char_code_at(raw, i);
    if (c < 48 || c > 57) break;
    status = status * 10 + (c - 48);
  }
  const sep = string.find(raw, "\r\n\r\n");
  if (sep < 0) return new Response(status, "", 0);
  const bodyStart = sep + 4;
  const bodyLen = n - bodyStart;
  return new Response(status, "", bodyLen);
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

describe("fixture:http_unified", () => {
  test("HTTP plain via fetch unificado", () => {
    const r = fetchUrl("http://httpforever.com/");
    expect(r.status == 200 ? "1" : "0").toBe("1");
    expect(r.bodyLen > 100 ? "1" : "0").toBe("1");
  });

  test("HTTPS via fetch unificado (mesma fn, scheme detectado)", () => {
    const r = fetchUrl("https://api.github.com/");
    expect(r.status == 200 ? "1" : "0").toBe("1");
    expect(r.bodyLen > 100 ? "1" : "0").toBe("1");
  });
});
