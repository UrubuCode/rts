// UDP roundtrip num unico processo. Usa string.starts_with em vez de
// String.prototype.indexOf — indexOf trava quando a string contem
// bytes nulos no meio (caso comum vindo de buffer.to_string com
// padding).
import { describe, test, expect } from "rts:test";
import { net, gc, buffer, string } from "rts";

describe("fixture:net_udp_echo", () => {
  test("send_to + recv_from + last_peer", () => {
    const server = net.udp_bind("127.0.0.1:51238");
    const client = net.udp_bind("127.0.0.1:0");

    const sent = net.udp_send_to(client, "127.0.0.1:51238", "udp-rts");
    const buf = buffer.alloc_zeroed(16);
    const got = net.udp_recv_from(server, buffer.ptr(buf), 16);
    const data = buffer.to_string(buf);
    const startsOk = string.starts_with(data, "udp-rts");
    buffer.free(buf);

    const peerH = net.udp_last_peer(server);
    const peerOk = peerH != 0;
    if (peerOk) gc.string_free(peerH);

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
