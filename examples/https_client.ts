// Cliente HTTPS via net.tcp_* + tls.client/send/recv.
//
// Conecta TCP em api.github.com:443, faz handshake TLS via tls.client(),
// manda GET /, recebe response encriptada, parseia status/headers/body.
//
// Uso:
//   target/release/rts.exe run examples/https_client.ts

import { net, tls, buffer, string, io, gc, thread } from "rts";

const HOST = "api.github.com";
const PATH = "/";

function main(): void {
  io.print("→ TCP connect " + HOST + ":443");
  const tcp = net.tcp_connect(HOST + ":443");
  if (tcp == 0) {
    io.eprint("FALHOU: tcp_connect\n");
    return;
  }
  io.print("  tcp ok");

  io.print("→ TLS handshake (SNI=" + HOST + ")");
  const stream = tls.client(tcp, HOST);
  if (stream == 0) {
    io.eprint("FALHOU: tls.client (handshake?)\n");
    return;
  }
  io.print("  tls ok (handshake completo)");

  let req = "";
  req = req + "GET " + PATH + " HTTP/1.1\r\n";
  req = req + "Host: " + HOST + "\r\n";
  req = req + "User-Agent: rts-tls/0.1\r\n";
  req = req + "Accept: */*\r\n";
  req = req + "Connection: close\r\n";
  req = req + "\r\n";

  io.print("→ enviando request (" + string.byte_len(req) + " bytes plain)");
  const sent = tls.send(stream, req);
  if (sent < 0) {
    io.eprint("FALHOU: tls.send\n");
    tls.close(stream);
    return;
  }
  io.print("  enviado " + sent + " bytes plain");

  thread.sleep_ms(500);

  io.print("→ recebendo response...");
  const buf = buffer.alloc_zeroed(8192);
  const n = tls.recv(stream, buffer.ptr(buf), 8192);
  io.print("  recebidos " + n + " bytes plain");
  tls.close(stream);

  if (n <= 0) {
    io.eprint("FALHOU: tls.recv\n");
    buffer.free(buf);
    return;
  }

  const raw = buffer.to_string(buf);
  const eol1 = string.find(raw, "\r\n");
  io.print("");
  io.print("─── STATUS ───────────────────────────────");
  io.print(sliceTo(raw, eol1));

  const sep = string.find(raw, "\r\n\r\n");
  if (sep > 0) {
    io.print("─── HEADERS ──────────────────────────────");
    io.print(sliceFromTo(raw, eol1 + 2, sep));

    const bodyStart = sep + 4;
    const bodyLen = n - bodyStart;
    io.print("─── BODY (" + bodyLen + " bytes) ─────────────────────");
    if (bodyLen > 400) {
      io.print(sliceFromTo(raw, bodyStart, bodyStart + 400));
      io.print("... [+" + (bodyLen - 400) + " bytes]");
    } else {
      io.print(sliceFromTo(raw, bodyStart, n));
    }
    io.print("─────────────────────────────────────────");
  }
  io.print("");
  io.print("✓ HTTPS via tls.* funcionou");

  buffer.free(buf);
}

main();

function sliceTo(s: string, end: number): string {
  let out = "";
  for (let i = 0; i < end; i = i + 1) out = out + string.char_at(s, i);
  return out;
}

function sliceFromTo(s: string, start: number, end: number): string {
  let out = "";
  for (let i = start; i < end; i = i + 1) out = out + string.char_at(s, i);
  return out;
}
