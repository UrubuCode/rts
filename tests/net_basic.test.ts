import { describe, test, expect } from "rts:test";
import { net, gc, buffer } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1) tcp_listen em porta efemera
const listener = net.tcp_listen("127.0.0.1:0");
if (listener == 0) {
  print("FAIL: tcp_listen retornou 0");
} else {
  print("listen-ok");
}

// 2) tcp_connect em endereco invalido — retorna 0
const cstream = net.tcp_connect("127.0.0.1:1");
if (cstream == 0) {
  print("connect-fail-ok");
} else {
  print("FAIL: connect deveria falhar");
  net.tcp_close(cstream);
}

// 3) tcp_close (void)
net.tcp_close(listener);
print("tcp-close-ok");

// 4) udp_bind
const sock = net.udp_bind("127.0.0.1:0");
if (sock == 0) {
  print("FAIL: udp_bind retornou 0");
} else {
  print("udp-bind-ok");
}

// 5) udp_recv_from sem dados disponiveis (socket so-bind, no remetente):
//    nao testamos recv pra evitar bloquear. Apenas valida last_peer pre-recv.
if (net.udp_last_peer(sock) == 0) {
  print("udp-no-peer-ok");
} else {
  print("FAIL: last_peer deveria ser 0");
}

// 6) udp_close (void)
net.udp_close(sock);
print("udp-close-ok");

// 7) resolve de "localhost" deve dar 127.0.0.1 ou ::1
const ipH = net.resolve("localhost");
if (ipH == 0) {
  print("FAIL: resolve(localhost) deu 0");
} else {
  print("resolve-ok");
  gc.string_free(ipH);
}

// 8) resolve de host invalido deve dar 0
const badH = net.resolve("invalid.host.that.does.not.exist.example");
if (badH == 0) {
  print("resolve-bad-ok");
} else {
  print("FAIL: resolve invalid deveria ser 0");
  gc.string_free(badH);
}

// 9) tcp_send pra stream invalido = -1
const sr = net.tcp_send(0, "hi");
if (sr == -1) {
  print("send-bad-ok");
} else {
  print("FAIL: send em stream 0 deveria ser -1");
}

// 10) tcp_recv com bufPtr=0 = -1
const rr = net.tcp_recv(0, 0, 16);
if (rr == -1) {
  print("recv-bad-ok");
} else {
  print("FAIL: recv com bufPtr 0 deveria ser -1");
}

describe("fixture:net_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "listen-ok\nconnect-fail-ok\ntcp-close-ok\nudp-bind-ok\nudp-no-peer-ok\nudp-close-ok\nresolve-ok\nresolve-bad-ok\nsend-bad-ok\nrecv-bad-ok\n"
    );
  });
});
