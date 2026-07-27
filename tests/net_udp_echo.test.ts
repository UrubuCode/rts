// UDP roundtrip num unico processo. Usa String.prototype.startsWith em vez de
// .indexOf — indexOf trava quando a string contem bytes nulos no meio (caso
// comum vindo de buffer.to_string com padding). Era string.starts_with ate o
// namespace rts:string ser drenado; startsWith e' a mesma operacao na
// value-class primordial e nao percorre a string alem do prefixo.
import { describe, test, expect } from "rts:test";
import { net, buffer } from "rts";

describe("fixture:net_udp_echo", () => {
  test("send_to + recv_from + last_peer", () => {
    // Porta dinamica em ambos pra evitar colisao com sockets
    // residuais. server bind primeiro, depois descobrir o endpoint.
    const server = net.udp_bind("127.0.0.1:9123");
    const client = net.udp_bind("127.0.0.1:0");

    const sent = net.udp_send_to(client, "127.0.0.1:9123", "udp-rts");
    const buf = buffer.alloc_zeroed(16);
    const got = net.udp_recv_from(server, buffer.ptr(buf), 16);
    const data = buffer.to_string(buf);
    const startsOk = data.startsWith("udp-rts");
    buffer.free(buf);

    const peerH = net.udp_last_peer(server);
    const peerOk = peerH != 0;

    net.udp_close(client);
    net.udp_close(server);

    expect(server != 0 ? "1" : "0").toBe("1");
    expect(client != 0 ? "1" : "0").toBe("1");
    expect(sent == 7 ? "1" : "0").toBe("1");
    expect(got == 7 ? "1" : "0").toBe("1");
    expect(startsOk ? "1" : "0").toBe("1");
    expect(peerOk ? "1" : "0").toBe("1");
  });
});
