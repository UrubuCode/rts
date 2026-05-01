// HTTP server via namespace `http_server` (actix-web por baixo).
//
// Diferente do `examples/http_server.ts` (raw TCP + parsing manual),
// aqui o parsing HTTP/1.1, keep-alive, thread pool, e dispatch sao
// feitos pelo actix-web. O usuario so registra o handler.
//
// Uso:
//   target/release/rts.exe run examples/http_actix.ts
//   abra http://127.0.0.1:8080/

import { http_server, io } from "rts";

io.print("RTS HTTP server (actix) escutando em http://127.0.0.1:8080/");
io.print("Ctrl+C pra parar.");

http_server.serve("127.0.0.1:8080", handler);

function handler(req: i64): void {
  const path = http_server.req_path(req);
  if (path == "/api/info") {
    http_server.respond(req, 200, "application/json",
      '{"server":"rts","version":"0.1","backend":"actix-web"}');
  } else if (path == "/") {
    http_server.respond(req, 200, "text/html; charset=utf-8",
      "<h1>RTS HTTP demo via actix-web</h1><p>Path: " + path + "</p><p><a href='/api/info'>/api/info</a></p>");
  } else {
    http_server.respond(req, 404, "text/plain", "not found: " + path);
  }
}
