// HTTP/1.1 server minimo em TS puro sobre o namespace `net`.
//
// Single-threaded (1 conexao por vez). Suficiente pra demonstrar o
// stack rodando ponta-a-ponta no browser.
//
// Para usar:
//   target/release/rts.exe run examples/http_server.ts
//   abra http://127.0.0.1:8080/  no browser
//   Ctrl+C pra parar

import { net, buffer, string, io } from "rts";

const ADDR = "127.0.0.1:8080";

const server = net.tcp_listen(ADDR);
if (server == 0) {
  io.eprint("FATAL: nao consegui bind em " + ADDR + " (porta ocupada?)\n");
} else {
  io.print("RTS HTTP server escutando em http://" + ADDR + "/");
  io.print("Ctrl+C pra parar.");

  while (true) {
    const client = net.tcp_accept(server);
    if (client == 0) {
      io.eprint("accept falhou; encerrando\n");
      break;
    }
    handleRequest(client);
    net.tcp_close(client);
  }

  net.tcp_close(server);
}

function handleRequest(client: number): void {
  const buf = buffer.alloc_zeroed(4096);
  const n = net.tcp_recv(client, buffer.ptr(buf), 4096);
  if (n <= 0) {
    buffer.free(buf);
    return;
  }

  const raw = buffer.to_string(buf);
  buffer.free(buf);

  // Parse linha 1: "METHOD PATH HTTP/1.1"
  const sp1 = string.find(raw, " ");
  const method = sp1 > 0 ? sliceTo(raw, sp1) : "GET";
  const rest = sliceFrom(raw, sp1 + 1);
  const sp2 = string.find(rest, " ");
  const path = sp2 > 0 ? sliceTo(rest, sp2) : "/";

  io.print("[req] " + method + " " + path);

  let status = 200;
  let body = "";
  let contentType = "text/html; charset=utf-8";

  if (path == "/") {
    body = renderHome();
  } else if (path == "/api/info") {
    contentType = "application/json";
    body = '{"server":"rts","version":"0.1","backend":"std::net"}';
  } else if (path == "/about") {
    body = renderAbout();
  } else {
    status = 404;
    body = render404(path);
  }

  sendResponse(client, status, contentType, body);
}

function renderHome(): string {
  let s = "";
  s = s + "<!DOCTYPE html>\n";
  s = s + "<html lang=\"pt-br\">\n";
  s = s + "<head>\n";
  s = s + "  <meta charset=\"utf-8\">\n";
  s = s + "  <title>RTS HTTP demo</title>\n";
  s = s + "  <style>\n";
  s = s + "    body { font-family: ui-sans-serif, system-ui; max-width: 720px; margin: 4rem auto; padding: 0 1rem; color: #1a1a1a; line-height: 1.55; }\n";
  s = s + "    h1 { font-size: 2rem; margin-bottom: 0.25rem; }\n";
  s = s + "    .tag { display: inline-block; padding: 0.15rem 0.5rem; background: #f0f0f0; border-radius: 4px; font-size: 0.8rem; color: #555; }\n";
  s = s + "    nav a { margin-right: 1rem; color: #0050b3; }\n";
  s = s + "    code { background: #f5f5f5; padding: 0.1rem 0.35rem; border-radius: 3px; }\n";
  s = s + "    .out { background: #0d1117; color: #c9d1d9; padding: 1rem; border-radius: 6px; font-family: ui-monospace, monospace; font-size: 0.9rem; min-height: 1.5rem; }\n";
  s = s + "    button { background: #0050b3; color: white; border: 0; padding: 0.5rem 1rem; border-radius: 4px; cursor: pointer; }\n";
  s = s + "    button:hover { background: #003a82; }\n";
  s = s + "  </style>\n";
  s = s + "</head>\n";
  s = s + "<body>\n";
  s = s + "  <h1>RTS HTTP demo <span class=\"tag\">v0.1</span></h1>\n";
  s = s + "  <p>Esta pagina foi servida por um HTTP server escrito em TypeScript puro, compilado pelo RTS para nativo, usando apenas o namespace <code>net</code> (std::net por baixo).</p>\n";
  s = s + "  <nav>\n";
  s = s + "    <a href=\"/\">home</a>\n";
  s = s + "    <a href=\"/about\">about</a>\n";
  s = s + "    <a href=\"/api/info\">/api/info</a>\n";
  s = s + "  </nav>\n";
  s = s + "  <h2>Teste a API</h2>\n";
  s = s + "  <p>Clica no botao abaixo - ele faz <code>fetch('/api/info')</code> e mostra a resposta:</p>\n";
  s = s + "  <button onclick=\"fetchInfo()\">GET /api/info</button>\n";
  s = s + "  <p class=\"out\" id=\"out\">aguardando...</p>\n";
  s = s + "  <script>\n";
  s = s + "    async function fetchInfo() {\n";
  s = s + "      const r = await fetch('/api/info');\n";
  s = s + "      const t = await r.text();\n";
  s = s + "      document.getElementById('out').textContent = `status=${r.status}  body=${t}`;\n";
  s = s + "    }\n";
  s = s + "  </script>\n";
  s = s + "</body>\n";
  s = s + "</html>\n";
  return s;
}

function renderAbout(): string {
  let s = "";
  s = s + "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>About</title>\n";
  s = s + "<style>body{font-family:ui-sans-serif,system-ui;max-width:720px;margin:4rem auto;padding:0 1rem;line-height:1.55}</style>\n";
  s = s + "</head><body>\n";
  s = s + "<h1>Sobre</h1>\n";
  s = s + "<p>Este servidor roda single-threaded sobre <code>net.tcp_listen/accept/recv/send</code>.</p>\n";
  s = s + "<p>O parser HTTP/1.1 e minimo: extrai METHOD e PATH da primeira linha, ignora headers/body, responde com Content-Length explicito + Connection: close pra simplificar.</p>\n";
  s = s + "<p><a href=\"/\">voltar</a></p>\n";
  s = s + "</body></html>\n";
  return s;
}

function render404(path: string): string {
  let s = "";
  s = s + "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>404</title>\n";
  s = s + "<style>body{font-family:ui-sans-serif,system-ui;max-width:720px;margin:4rem auto;padding:0 1rem;line-height:1.55}</style>\n";
  s = s + "</head><body>\n";
  s = s + "<h1>404 - nao encontrado</h1>\n";
  s = s + "<p>O path <code>" + path + "</code> nao existe.</p>\n";
  s = s + "<p><a href=\"/\">home</a></p>\n";
  s = s + "</body></html>\n";
  return s;
}

function sendResponse(client: number, status: number, contentType: string, body: string): void {
  const statusText = statusReason(status);
  const contentLen = string.byte_len(body);
  let header = "";
  header = header + "HTTP/1.1 " + status + " " + statusText + "\r\n";
  header = header + "Content-Type: " + contentType + "\r\n";
  header = header + "Content-Length: " + contentLen + "\r\n";
  header = header + "Connection: close\r\n";
  header = header + "\r\n";
  net.tcp_send(client, header);
  net.tcp_send(client, body);
}

function statusReason(code: number): string {
  if (code == 200) return "OK";
  if (code == 404) return "Not Found";
  if (code == 500) return "Internal Server Error";
  return "OK";
}

function sliceTo(s: string, end: number): string {
  let out = "";
  for (let i = 0; i < end; i = i + 1) {
    out = out + string.char_at(s, i);
  }
  return out;
}

function sliceFrom(s: string, start: number): string {
  let out = "";
  const n = string.char_count(s);
  for (let i = start; i < n; i = i + 1) {
    out = out + string.char_at(s, i);
  }
  return out;
}
