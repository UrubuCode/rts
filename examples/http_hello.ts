import { io } from "rts";
import { net } from "rts";

// Servidor HTTP/1.1 minimo: aceita uma conexao, le a request,
// responde com um texto simples e encerra. Nao tem loop — para
// testar, rode em uma shell e use curl em outra:
//
//   shell 1:  rts run examples/http_hello.ts
//   shell 2:  curl -v http://127.0.0.1:3000/hello
//
// O servidor encerra automaticamente apos a primeira resposta.

function main(): void {
  const listenResult = net.tcp_listen("127.0.0.1:3000");
  if (!listenResult.ok) {
    io.print("failed to listen: " + listenResult.error);
    return;
  }
  const listener = listenResult.value;
  io.print("listening on http://127.0.0.1:3000");
  io.print("try: curl http://127.0.0.1:3000/hello");

  const acceptResult = net.tcp_accept(listener);
  if (!acceptResult.ok) {
    io.print("accept failed: " + acceptResult.error);
    return;
  }
  const stream = acceptResult.value.stream;

  const reqResult = net.http_read_request(stream);
  if (!reqResult.ok) {
    io.print("read_request failed: " + reqResult.error);
    net.tcp_shutdown(stream, "Both");
    return;
  }
  const req = reqResult.value;

  const methodResult = net.http_request_method(req);
  const pathResult = net.http_request_path(req);
  const method = methodResult.value;
  const path = pathResult.value;

  io.print("received: " + method + " " + path);

  const body = "hello from rts http server\nyou requested: " + method + " " + path + "\n";
  const writeResult = net.http_response_write(stream, 200, body);
  if (!writeResult.ok) {
    io.print("write failed: " + writeResult.error);
  } else {
    io.print("response sent (" + writeResult.value + " bytes)");
  }

  net.http_request_free(req);
  net.tcp_shutdown(stream, "Both");
}
