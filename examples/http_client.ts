// Cliente HTTP/1.1 minimo em TS puro sobre o namespace `net`.
//
// Conecta em httpforever.com:80 (HTTP plain — nosso `net` nao tem
// TLS), manda GET /, recebe a response, parseia status/headers/body.
//
// Uso:
//   target/release/rts.exe run examples/http_client.ts
//
// Sites HTTPS-only nao funcionam — TLS sera coberto na issue #234.

import { net, buffer, string, io, gc, thread } from "rts";

const HOST = "httpforever.com";

function main(): void {
  io.print("→ resolvendo " + HOST + "...");
  const ipH = net.resolve(HOST);
  if (ipH == 0) {
    io.eprint("FALHOU: nao resolveu " + HOST + "\n");
    return;
  }
  io.print("  ip ok (handle=" + gc.string_from_i64(ipH) + ")");
  gc.string_free(ipH);

  io.print("→ conectando em " + HOST + ":80");
  const stream = net.tcp_connect(HOST + ":80");
  if (stream == 0) {
    io.eprint("FALHOU: connect\n");
    return;
  }
  io.print("  conectado");

  let req = "";
  req = req + "GET / HTTP/1.1\r\n";
  req = req + "Host: " + HOST + "\r\n";
  req = req + "User-Agent: rts-net/0.1\r\n";
  req = req + "Accept: */*\r\n";
  req = req + "Connection: close\r\n";
  req = req + "\r\n";

  io.print("→ enviando request (" + string.byte_len(req) + " bytes)");
  const sent = net.tcp_send(stream, req);
  if (sent < 0) {
    io.eprint("FALHOU: send\n");
    net.tcp_close(stream);
    return;
  }

  // Da tempo da response chegar.
  thread.sleep_ms(300);

  // Le ate 8 KB. Loop com offset esbarra em bug de codegen
  // (aritmetica em u64 ptr passada como U64 extern arg corrompe
  // bits altos no Windows). Por enquanto, 1 chamada — suficiente
  // pra responses pequenas.
  io.print("→ recebendo response...");
  const buf = buffer.alloc_zeroed(8192);
  const n = net.tcp_recv(stream, buffer.ptr(buf), 8192);
  io.print("  recebidos " + n + " bytes");
  net.tcp_close(stream);

  if (n <= 0) {
    io.eprint("FALHOU: recv\n");
    buffer.free(buf);
    return;
  }

  const raw = buffer.to_string(buf);

  const eol1 = string.find(raw, "\r\n");
  const statusLine = sliceTo(raw, eol1);
  io.print("");
  io.print("─── STATUS ───────────────────────────────");
  io.print(statusLine);

  const sep = string.find(raw, "\r\n\r\n");
  if (sep < 0) {
    io.eprint("FALHOU: nao achou fim dos headers\n");
    buffer.free(buf);
    return;
  }
  const headersBlock = sliceFromTo(raw, eol1 + 2, sep);
  io.print("─── HEADERS ──────────────────────────────");
  io.print(headersBlock);

  const bodyStart = sep + 4;
  const bodyEnd = n;
  const bodyLen = bodyEnd - bodyStart;
  const body = sliceFromTo(raw, bodyStart, bodyEnd);
  io.print("─── BODY (" + bodyLen + " bytes) ─────────────────────");
  if (bodyLen > 400) {
    io.print(sliceTo(body, 400));
    io.print("... [+" + (bodyLen - 400) + " bytes]");
  } else {
    io.print(body);
  }
  io.print("─────────────────────────────────────────");
  io.print("");
  io.print("✓ request HTTP via net.tcp_* funcionou");

  buffer.free(buf);
}

main();

function sliceTo(s: string, end: number): string {
  let out = "";
  for (let i = 0; i < end; i = i + 1) {
    out = out + string.char_at(s, i);
  }
  return out;
}

function sliceFromTo(s: string, start: number, end: number): string {
  let out = "";
  for (let i = start; i < end; i = i + 1) {
    out = out + string.char_at(s, i);
  }
  return out;
}
