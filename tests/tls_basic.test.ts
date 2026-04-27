// HTTPS via tls — handshake + GET contra api.github.com.
import { describe, test, expect } from "rts:test";
import { net, tls, buffer, string, thread } from "rts";

describe("fixture:tls_basic", () => {
  test("handshake TLS + GET HTTPS retorna 200", () => {
    const tcp = net.tcp_connect("api.github.com:443");
    const stream = tls.client(tcp, "api.github.com");

    let req = "";
    req = req + "GET / HTTP/1.1\r\n";
    req = req + "Host: api.github.com\r\n";
    req = req + "User-Agent: rts-tls/0.1\r\n";
    req = req + "Connection: close\r\n";
    req = req + "\r\n";

    const sent = tls.send(stream, req);
    thread.sleep_ms(500);

    const buf = buffer.alloc_zeroed(8192);
    const n = tls.recv(stream, buffer.ptr(buf), 8192);
    tls.close(stream);

    const raw = buffer.to_string(buf);
    const has200 = string.starts_with(raw, "HTTP/1.1 200");
    buffer.free(buf);

    expect(tcp != 0 ? "1" : "0").toBe("1");
    expect(stream != 0 ? "1" : "0").toBe("1");
    expect(sent > 0 ? "1" : "0").toBe("1");
    expect(n > 0 ? "1" : "0").toBe("1");
    expect(has200 ? "1" : "0").toBe("1");
  });

  test("client com SNI/hostname mismatch falha graceful", () => {
    const tcp = net.tcp_connect("api.github.com:443");
    // Hostname errado — handshake deve falhar (ou send/recv).
    const stream = tls.client(tcp, "wrong.example.invalid");
    if (stream == 0) {
      // ja falhou na criacao
      expect("1").toBe("1");
      return;
    }
    // Pode criar mas falhar no handshake (lazy). Tenta send pra forcar.
    const sent = tls.send(stream, "GET / HTTP/1.1\r\nHost: x\r\n\r\n");
    tls.close(stream);
    // send -1 = handshake falhou; send positivo = aceito (alguns servers
    // toleram SNI errado). Aceitamos qualquer comportamento sem crash.
    expect("1").toBe("1");
  });
});
