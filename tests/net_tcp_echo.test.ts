// TCP echo end-to-end via subprocess. Toda a logica vive dentro do
// test() pra alinhar com o padrao dos outros testes que usam
// process.spawn (vide abstract_class_no_new_err.test.ts).
import { describe, test, expect } from "rts:test";
import { net, gc, buffer, process, thread, fs } from "rts";

function resolveRtsExe(): string {
  if (fs.exists("target/release/rts.exe") === 1) return "target/release/rts.exe";
  if (fs.exists("target/debug/rts.exe") === 1) return "target/debug/rts.exe";
  if (fs.exists("target/release/rts") === 1) return "target/release/rts";
  if (fs.exists("target/debug/rts") === 1) return "target/debug/rts";
  return "rts";
}

describe("fixture:net_tcp_echo", () => {
  test("client RTS faz echo roundtrip contra server RTS subprocess", () => {
    const exe = resolveRtsExe();
    const child = process.spawn(exe, "run\ntests/_net_echo_server.ts");

    // Server precisa de tempo pra bind+listen.
    thread.sleep_ms(300);

    const client = net.tcp_connect("127.0.0.1:51237");
    const sent = net.tcp_send(client, "rts-echo");

    const rxbuf = buffer.alloc_zeroed(8);
    const got = net.tcp_recv(client, buffer.ptr(rxbuf), 8);
    const echoed = buffer.to_string(rxbuf);
    buffer.free(rxbuf);
    net.tcp_close(client);

    const exitCode = process.wait(child);

    expect(client != 0 ? "1" : "0").toBe("1");
    expect(sent == 8 ? "1" : "0").toBe("1");
    expect(got == 8 ? "1" : "0").toBe("1");
    expect(echoed).toBe("rts-echo");
    expect(exitCode == 0 ? "1" : "0").toBe("1");
  });
});
